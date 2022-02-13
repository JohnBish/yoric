use std::collections::HashMap;
use bincode;
use tokio::sync::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use futures::stream::{self, futures_unordered::FuturesUnordered};
use tokio_tungstenite::connect_async;
use tungstenite::{Message, Result, Error};
use yoric_core::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Topic {
    State,
    Error,
    Info,
    Chat
}

#[derive(Debug)]
pub enum ClientCmd {
    Send(ClientMessage),
    Subscribe(Topic, mpsc::Sender<ServerMessage>)
}

#[derive(Clone)]
pub struct ClientHandle {
    sender: mpsc::UnboundedSender<ClientCmd>,
}

impl ClientHandle {
    pub fn new(exit_listener: oneshot::Receiver<Result<()>>) -> ClientHandle {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle_tx = tx.clone();
        let mut client = Client {
            sender: tx,
            receiver: rx,
            exit_listener,
            subscribers: HashMap::new()
        };

        tokio::spawn(async move { client.run().await });

        ClientHandle { sender: handle_tx }
    }

    pub fn send(&mut self, msg: ClientMessage) {
        self.sender.send(ClientCmd::Send(msg)).unwrap();
    }

    pub fn subscribe(&mut self, topic: Topic) -> mpsc::Receiver<ServerMessage> {
        let (sender, receiver) = mpsc::channel(32);
        self.sender.send(ClientCmd::Subscribe(topic, sender)).unwrap();
        receiver
    }
}

struct Client {
    sender: mpsc::UnboundedSender<ClientCmd>,
    receiver: mpsc::UnboundedReceiver<ClientCmd>,
    exit_listener: oneshot::Receiver<Result<()>>,
    subscribers: HashMap<Topic, Vec<mpsc::Sender<ServerMessage>>>
}

impl Client {
    pub fn send(&self, msg: ClientMessage) {
        info!("Sending message: {:?}", msg);
        match self.sender.send(ClientCmd::Send(msg)) {
            Ok(_) => (),
            Err(e) => warn!("{:?}", e)
        }
    }

    async fn notify_topic(&mut self, msg: ServerMessage, topic: Topic) {
        if let Some(subs) = self.subscribers.get_mut(&topic) {
            *subs = stream::iter(subs.iter())
                .filter_map(|sub| {
                    let msg = msg.clone();
                    async move {
                        match sub.send(msg.clone()).await {
                            Ok(_) => Some(sub.clone()),
                            Err(_) => { info!("Failed to notify subscriber; dropping handle"); None }
                        }
                    }
                })
                .collect::<FuturesUnordered<_>>()
                .await
                .into_iter()
                .collect::<Vec<_>>()
        }
    }

    async fn notify_subscribers(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::PartialState(_) => self.notify_topic(msg, Topic::State).await,
            ServerMessage::InvalidReq => self.notify_topic(msg, Topic::Error).await,
            ServerMessage::LobbyID(_) => self.notify_topic(msg, Topic::Info).await,
            ServerMessage::Lobby(_) => self.notify_topic(msg, Topic::Info).await,
            ServerMessage::Starting => self.notify_topic(msg, Topic::Info).await,
            ServerMessage::Chat(_, _) => self.notify_topic(msg, Topic::Chat).await
        }
    }

    async fn handle_msg(&mut self, msg: Option<Result<Message, Error>>) {
        match msg {
            Some(msg) => match msg {
                Ok(msg) => {
                    if let Ok(msg) = bincode::deserialize::<ServerMessage>(&msg.into_data()[..]) {
                        info!("Received: {:?}", &msg);
                        info!("Notifying subscribers");
                        self.notify_subscribers(msg).await;
                    }
                }
                Err(e) => warn!("{}", e)
            },
            None => ()
        }

    }

    pub fn subscribe(&mut self, topic: Topic, sender: mpsc::Sender<ServerMessage>) {
        if self.subscribers.contains_key(&topic) {
            self.subscribers.get_mut(&topic).unwrap().push(sender);
        } else {
            self.subscribers.insert(topic, vec![ sender ]);
        }
    }

    pub async fn run(&mut self) {
        let connect_addr = "ws://192.168.2.22:8000";

        let url = url::Url::parse(&connect_addr).unwrap();

        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
        info!("WebSocket handshake has been successfully completed");
        let (mut writer, mut reader) = ws_stream.split();

        loop {
            tokio::select!{
                // Listen for interrupt
                ecode = &mut self.exit_listener => match ecode {
                    Ok(Ok(())) => break,
                    Ok(Err(e)) => {
                        warn!("{}", e);
                        break;
                    },
                    _ => break
                },
                // Listen for requests to send outgoing messages and to register to topics
                msg = self.receiver.recv() => match msg {
                    Some(ClientCmd::Send(msg)) => {
                        let encoded = Message::binary(bincode::serialize(&msg).unwrap());
                        writer.send(encoded).await.unwrap();
                    },
                    Some(ClientCmd::Subscribe(topic, sender)) => self.subscribe(topic, sender),
                    _ => ()
                },
                // Listen on incoming messages
                msg = reader.next() => self.handle_msg(msg).await
            }
        }
    }
}

