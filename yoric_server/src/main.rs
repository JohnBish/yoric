#![feature(map_first_last)]
#![allow(dead_code)]

mod game;

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use bincode;
use uuid::Uuid;
use rand::Rng;
use futures_util::{
    SinkExt, StreamExt,
    future::join_all
};
use std::net::SocketAddr;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
    time::sleep
};
use tokio_tungstenite::{accept_async, tungstenite::Error};
use tungstenite::{Message, Result};
use yoric_core::*;

#[derive(Debug)]
struct Connection {
    id: Uuid,
    addr: SocketAddr,
    stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
}

impl Connection {
    fn from_stream(stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, addr: SocketAddr) -> Connection {
        Connection {
            id: Uuid::new_v4(),
            addr,
            stream,
        }
    }

    async fn next(&mut self) -> Result<Message, Error> {
        self.stream.next().await.unwrap()
    }

    async fn send(&mut self, msg: Message) {
        self.stream.send(msg).await.unwrap();
    }

    async fn send_msg(&mut self, msg: ServerMessage) {
        let encoded: Vec<u8> = bincode::serialize(&msg).unwrap();
        self.send(Message::binary(encoded)).await;
    }

    async fn next_msg(&mut self) -> Result<ClientMessage, Error> {
        loop {
            let msg = self.next().await;

            match msg {
                Ok(msg) => {
                    if msg.is_binary() {
                        let data = msg.into_data();
                        println!("Received: {:?}", data);
                        if let Ok(msg) = bincode::deserialize::<ClientMessage>(&data[..]) {
                            println!("Decoded: {:?}", msg);
                            return Ok(msg);
                        } else {
                            self.send_msg(ServerMessage::InvalidReq).await;
                        }
                    } else {
                        self.send_msg(ServerMessage::InvalidReq).await;
                    }
                },
                Err(e) => return Err(e)
            }
        }
    }

    async fn next_join(&mut self) -> Result<(JoinRequest, String)> {
        loop {
            let msg = self.next_msg().await;

            match msg {
                Ok(ClientMessage::Join(jr, uname)) => return Ok((jr, uname)),
                Err(e) => return Err(e),
                _ => self.send_msg(ServerMessage::InvalidReq).await,
            }
        }
    }

    async fn next_do(&mut self) -> Result<Command> {
        loop {
            let msg = self.next_msg().await;

            match msg {
                Ok(ClientMessage::Do(comm)) => return Ok(comm),
                Err(e) => return Err(e),
                _ => self.send_msg(ServerMessage::InvalidReq).await,
            }
        }
    }
}

#[derive(Debug)]
pub struct ClientHandle {
    id: Uuid,
    uname: String,
    sender: mpsc::Sender<ServerMessage>,
    is_host: bool
}

#[derive(Debug)]
struct LobbyHandle {
    id: String,
    sender: mpsc::Sender<(Uuid, ClientMessage)>,
    receiver: mpsc::Receiver<ServerMessage>
}

#[derive(Debug)]
struct Lobby {
    id: String,
    game_state: GameState,
    connections: BTreeMap<Uuid, ClientHandle>,
    reg_channel: mpsc::Receiver<(Uuid, String, oneshot::Sender<LobbyHandle>)>,
    client_channel: mpsc::Receiver<(Uuid, ClientMessage)>,
    client_handle_sender: mpsc::Sender<(Uuid, ClientMessage)> // For cloning and distributing to clients
}

impl Lobby {
    fn new(id: String) -> (Lobby, mpsc::Sender<(Uuid, String, oneshot::Sender<LobbyHandle>)>) {
        let (txr, rxr) = mpsc::channel(32); // For registration
        let (txc, rxc) = mpsc::channel(32); // For messages from client
        (Lobby {
            id,
            game_state: GameState::new(),
            connections: BTreeMap::new(),
            reg_channel: rxr,
            client_channel: rxc,
            client_handle_sender: txc
        }, txr)
    }

    fn register_client(&mut self, id: Uuid, uname: String) -> LobbyHandle {
        let (tx, rx) = mpsc::channel(32);

        self.connections.insert(
            id.clone(),
            ClientHandle {
                id,
                uname: uname.clone(),
                sender: tx,
                is_host: self.connections.is_empty()
            }
        );

        self.game_state.add_player(uname);

        LobbyHandle {
            id: self.id.clone(),
            sender: self.client_handle_sender.clone(),
            receiver: rx
        }
    }

    async fn drop_client(&mut self, id: Uuid) {
        println!("Dropping client {} from lobby {}", &id, &self.id);

        let conn = self.connections.remove(&id).unwrap();

        if !self.connections.is_empty() {
            if conn.is_host {
                self.connections
                    .first_entry()
                    .unwrap()
                    .into_mut()
                    .is_host = true;
            }
        }

        self.send_lobby_info().await;
    }

    fn random_id() -> String {
        const LOBBYID_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                       0123456789";
        const LOBBYID_LEN: usize = 4;
        let mut rng = rand::thread_rng();

        (0..LOBBYID_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..LOBBYID_CHARSET.len());
                LOBBYID_CHARSET[idx] as char
            })
            .collect()
    }

    async fn send_lobby_info(&self) {
        let mut host = String::from("");
        let unames = self.connections
            .iter()
            .filter_map(|(_, h)| {
                if h.is_host {
                    host = h.uname.clone();
                    None
                }
                else {
                    Some(h.uname.clone())
                }
            })
            .collect();

        let lobby_state = LobbyState {
           count: self.connections.len(),
           unames,
           host
        };

        self.notify_all(ServerMessage::Lobby(lobby_state)).await;
    }

    async fn send_partial_states(&self) {
        join_all(self.connections
            .iter()
            .map(|(_, handle)| {
                let uname = &handle.uname;
                let partial_state = self.game_state.partial(uname);
                handle.sender.send(ServerMessage::PartialState(partial_state))
            })
        ).await;
    }

    async fn notify_all(&self, msg: ServerMessage) {
        join_all(self.connections.iter()
            .map(|(_, client)| {
                client.sender.send(msg.clone())
            })
        ).await;
    }

    async fn run(&mut self) {
        println!("Starting lobby with id {}", self.id);
        loop {
            tokio::select! {
                Some((id, uname, stub_sender)) = self.reg_channel.recv() => {
                    stub_sender.send(self.register_client(id, uname)).unwrap();

                    // I really don't like this solution but it'll do for now. In practice, it's
                    // fine since two clients registering to this lobby at the same time will just
                    // be processed 10ms apart
                    sleep(Duration::from_millis(10)).await; // Need to make sure client has time to process ID
                    self.send_lobby_info().await;
                },
                Some((id, msg)) = self.client_channel.recv() => {
                    println!("New message {:?} from {:?}", msg, id);

                    match msg {
                        ClientMessage::Leave | ClientMessage::Dead => {
                            if self.connections.contains_key(&id) {
                                self.drop_client(id).await;
                            }

                            if self.connections.is_empty() {
                                break;
                            }
                        },
                        ClientMessage::Start => {
                            self.game_state.phase = Phase::Play;
                            self.notify_all(ServerMessage::Starting).await;
                            sleep(Duration::from_millis(10)).await;
                            self.send_partial_states().await;
                        },
                        ClientMessage::Do(cmd) => {
                            let (result, msg) = game::handle_action(cmd, id, &mut self.game_state, &self.connections).await;
                            if result {
                                self.send_partial_states().await;
                                if let Some(msg) = msg {
                                    self.notify_all(ServerMessage::Chat(String::from("Server"), msg)).await;
                                }
                            }
                        },
                        ClientMessage::Chat(msg) => {
                            let caller_handle = self.connections.get(&id).unwrap();
                            let uname = caller_handle.uname.clone();
                            self.notify_all(ServerMessage::Chat(uname, msg)).await;
                        }
                        _ => unimplemented!()
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct State {
    lobbies: HashMap<String, mpsc::Sender<(Uuid, String, oneshot::Sender<LobbyHandle>)>>
}

impl State {
    async fn new_lobby(&mut self, client_id: Uuid, uname: String, tx: mpsc::Sender<StateCmd>) -> Option<LobbyHandle> {
        let mut id = Lobby::random_id();
        while self.lobbies.contains_key(&id) {
            id = Lobby::random_id();
        }
        let (mut lobby, reg_sender) = Lobby::new(id.clone());

        if self.lobbies.contains_key(&id) { return None };
        self.lobbies.insert(id.clone(), reg_sender);
        println!("Created new lobby with ID {}", &id);

        tokio::spawn(async move {
            lobby.run().await;
            println!("Lobby with ID {} terminated", &lobby.id);
            tx.send(StateCmd::RemoveLobby{ lobby_id: lobby.id }).await.unwrap(); // Remove id/sender from state
        });
        self.register_to_lobby(id, client_id, uname).await
    }

    async fn register_to_lobby(&mut self, lobby_id: String, client_id: Uuid, uname: String) -> Option<LobbyHandle> {
        let (tx, rx) = oneshot::channel();
        match self.lobbies.get_mut(&lobby_id) {
            Some(reg_sender) => {
                reg_sender
                    .send((client_id, uname, tx))
                    .await.unwrap();
                return Some(rx.await.unwrap())
            },
            None => return None
        }
    }

    fn remove_lobby(&mut self, lobby_id: String) {
        self.lobbies.remove(&lobby_id);
    }
}

#[derive(Debug)]
enum StateCmd {
    CreateLobby{ client_id: Uuid, uname: String, rec: oneshot::Sender<Option<LobbyHandle>> },
    PushConnection{ lobby_id: String, client_id: Uuid, uname: String, rec: oneshot::Sender<Option<LobbyHandle>> },
    RemoveLobby{ lobby_id: String }
}

async fn handle_connection(peer: SocketAddr, stream: TcpStream, tx: mpsc::Sender<StateCmd>) {
    let ws_stream = accept_async(stream).await.unwrap();
    println!("Connection to {} succeeded!", peer);

    let mut conn = Connection::from_stream(ws_stream, peer);

    loop {
        let lobby_handle: Option<LobbyHandle>;
        loop {
            let req = conn.next_join().await;
            let (ls_tx, ls_rx) = oneshot::channel::<Option<LobbyHandle>>();

            match req {
                Ok((JoinRequest::NewLobby, uname)) => tx.send(StateCmd::CreateLobby{
                        client_id: conn.id.clone(),
                        uname,
                        rec: ls_tx
                    }).await.unwrap(),
                Ok((JoinRequest::JoinLobby(id), uname)) => tx.send(StateCmd::PushConnection {
                    lobby_id: id,
                    client_id: conn.id.clone(),
                    uname,
                    rec: ls_tx
                }).await.unwrap(),
                Err(e) => {eprintln!("{}", e); return} // Client Disconnect without completing handshake
            }

            if let Ok(Some(handle)) = ls_rx.await {
                lobby_handle = Some(handle);
                break
            }
        }
        let mut lobby_handle = lobby_handle.unwrap();

        println!("Sending lobby ID to client");
        conn.send_msg(ServerMessage::LobbyID(lobby_handle.id.clone())).await; // Send client the lobby id so others can join

        // Forward all new messages to lobby, and forward all messages from lobby to client
        loop {
            tokio::select! {
                client_msg_opt = conn.next_msg() => {
                    match client_msg_opt {
                        Ok(client_msg) => {
                            match client_msg {
                                ClientMessage::Leave => {
                                    lobby_handle.sender.send((conn.id.clone(), ClientMessage::Leave)).await.unwrap();
                                    break;
                                },
                                msg => lobby_handle.sender.send((conn.id.clone(), msg)).await.unwrap()
                            }
                        },
                        Err(e) => { // Client Disconnect without completing handshake
                            eprintln!("{}", e);
                            lobby_handle.sender.send((conn.id.clone(), ClientMessage::Dead)).await.unwrap();
                            return;
                        }
                    };
                },
                Some(lobby_msg) = lobby_handle.receiver.recv() =>
                    conn.send_msg(lobby_msg).await
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State { lobbies: HashMap::new() };
    let (tx, mut rx) = mpsc::channel::<StateCmd>(32);

    // Spawn state manager
    let txm = tx.clone();
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                StateCmd::CreateLobby { client_id, uname, rec } =>
                    rec.send(state.new_lobby(client_id, uname, txm.clone()).await).unwrap(), // Create lobby and send handle
                StateCmd::PushConnection { lobby_id, client_id, uname, rec } =>
                    rec.send(state.register_to_lobby(lobby_id, client_id, uname).await).unwrap(), // Join lobby and send handle
                StateCmd::RemoveLobby { lobby_id } =>
                    state.remove_lobby(lobby_id)
            }
        }
    });

    let addr = "192.168.2.22:8000";
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let peer = stream.peer_addr()?;
        println!("Connection from peer address: {}", peer);

        tokio::spawn(handle_connection(peer, stream, tx.clone()));
    }

    Ok(())
}
