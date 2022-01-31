use std::sync::Arc;
use std::collections::HashMap;
use bincode;
use uuid::Uuid;
use rand::Rng;
use futures_util::{Stream, SinkExt, StreamExt, future::select_all, FutureExt};
use std::net::SocketAddr;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot, Mutex}
};
use tokio_tungstenite::{accept_async, tungstenite::Error};
use tungstenite::{Message, Result};
use yoric_core::*;

#[derive(Debug)]
struct Connection {
    id: Uuid,
    addr: SocketAddr,
    stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>
}

impl Connection {
    fn from_stream(stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, addr: SocketAddr) -> Connection {
        Connection {
            id: Uuid::new_v4(),
            addr,
            stream
        }
    }

    async fn next(&mut self) -> Message {
        self.stream.next().await.unwrap().unwrap()
    }

    async fn send(&mut self, msg: Message) {
        self.stream.send(msg).await.unwrap();
    }

    async fn send_msg(&mut self, msg: ServerMessage) {
        let encoded: Vec<u8> = bincode::serialize(&msg).unwrap();
        self.send(Message::binary(encoded)).await;
    }

    async fn next_msg(&mut self) -> ClientMessage {
        loop {
            let msg = self.next().await;

            if msg.is_binary() {
                let data = msg.into_data();
                println!("Received: {:?}", data);
                if let Ok(msg) = bincode::deserialize::<ClientMessage>(&data[..]) {
                    println!("Decoded: {:?}", msg);
                    return msg;
                } else {
                    self.send_msg(ServerMessage::InvalidReq).await;
                }
            } else {
                self.send_msg(ServerMessage::InvalidReq).await;
            }
        }
    }

    async fn next_join(&mut self) -> JoinRequest {
        loop {
            let msg = self.next_msg().await;

            match msg {
                ClientMessage::Join(jr) => return jr,
                _ => self.send_msg(ServerMessage::InvalidReq).await,
            }
        }
    }

    async fn next_do(&mut self) -> Command {
        loop {
            let msg = self.next_msg().await;

            match msg {
                ClientMessage::Do(comm) => return comm,
                _ => self.send_msg(ServerMessage::InvalidReq).await,
            }
        }
    }
}

#[derive(Debug)]
struct ClientStub {
    id: Uuid,
    sender: mpsc::Sender<ServerMessage>,
}

#[derive(Debug)]
struct LobbyStub {
    id: String,
    sender: mpsc::Sender<(Uuid, ClientMessage)>,
    receiver: mpsc::Receiver<ServerMessage>
}

#[derive(Debug)]
struct Lobby {
    id: String,
    game_state: GameState,
    connections: Vec<ClientStub>,
    reg_channel: mpsc::Receiver<(Uuid, oneshot::Sender<LobbyStub>)>,
    client_channel: mpsc::Receiver<(Uuid, ClientMessage)>,
    client_stub_sender: mpsc::Sender<(Uuid, ClientMessage)> // For cloning and distributing to clients
}

impl Lobby {
    fn new(id: String) -> (Lobby, mpsc::Sender<(Uuid, oneshot::Sender<LobbyStub>)>) {
        let (txr, rxr) = mpsc::channel(32); // For registration
        let (txc, rxc) = mpsc::channel(32); // For messages from client
        (Lobby {
            id,
            game_state: GameState { },
            connections: vec![],
            reg_channel: rxr,
            client_channel: rxc,
            client_stub_sender: txc
        }, txr)
    }

    // Registers client and returns stub
    fn register_client(&mut self, id: Uuid) -> LobbyStub {
        let (tx, rx) = mpsc::channel(32);

        self.connections.push(ClientStub {
            id,
            sender: tx,
        });
        
        LobbyStub {
            id: self.id.clone(),
            sender: self.client_stub_sender.clone(),
            receiver: rx
        }
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

    async fn run(&mut self) {
        println!("Starting lobby with id {}", self.id);
        loop {
            tokio::select! {
                Some((id, stub_sender)) = self.reg_channel.recv() => {
                    stub_sender.send(self.register_client(id)).unwrap();
                },
                Some((id, msg)) = self.client_channel.recv() => {
                    println!("New message {:?} from {:?}", msg, id);
                }
            }
        }
    }
}

#[derive(Debug)]
struct LobbyHandle {
    lobby: Arc<Mutex<Lobby>>, // This should not be locked unless run
    reg_sender: mpsc::Sender<(Uuid, oneshot::Sender<LobbyStub>)>
}

#[derive(Debug)]
struct State {
    lobbies: HashMap<String, LobbyHandle>
}

impl State {
    async fn new_lobby(&mut self, client_id: Uuid) -> LobbyStub {
        let mut id = Lobby::random_id();
        while self.lobbies.contains_key(&id) {
            id = Lobby::random_id();
        }
        let (lobby, reg_sender) = Lobby::new(id.clone());
        self.lobbies.insert(id.clone(), LobbyHandle{ lobby: Arc::new(Mutex::new(lobby)), reg_sender });
        println!("Created new lobby with id {}", &id);
        
        self.run_lobby(id.clone());
        self.register_to_lobby(id, client_id).await
    }

    async fn register_to_lobby(&mut self, lobby_id: String, client_id: Uuid) -> LobbyStub {
        let (tx, rx) = oneshot::channel();
        self.lobbies.get_mut(&lobby_id)
            .unwrap()
            .reg_sender.send((client_id, tx))
            .await.unwrap();

        rx.await.unwrap()
    }

    fn run_lobby(&mut self, lobby_id: String) {
        if let Some(lobbyh) = self.lobbies.get_mut(&lobby_id) {
            let guard = lobbyh.lobby.clone();
            tokio::spawn(async move {
                let mut lobby = guard.lock().await;
                lobby.run().await;
            });
        }
    }
}

#[derive(Debug)]
enum StateCmd {
    CreateLobby{ client_id: Uuid, tx: oneshot::Sender<LobbyStub> },
    PushConnection{ lobby_id: String, client_id: Uuid, tx: oneshot::Sender<LobbyStub> },
}

async fn handle_connection(peer: SocketAddr, stream: TcpStream, tx: mpsc::Sender<StateCmd>) {
    let ws_stream = accept_async(stream).await.unwrap();
    println!("Connection to {} succeeded!", peer);

    let mut conn = Connection::from_stream(ws_stream, peer);

    let req = conn.next_join().await;
    let (ls_tx, ls_rx) = oneshot::channel::<LobbyStub>();

    match req {
        JoinRequest::NewLobby => {
            tx.send(StateCmd::CreateLobby{
                client_id: conn.id.clone(),
                tx: ls_tx
            }).await.unwrap();
        },
        JoinRequest::JoinLobby(id) => tx.send(StateCmd::PushConnection {
            lobby_id: id,
            client_id: conn.id.clone(),
            tx: ls_tx
        }).await.unwrap()
    }

    let mut lobby_stub = ls_rx.await.unwrap();
    println!("Sending lobby ID to client");
    conn.send_msg(ServerMessage::LobbyID(lobby_stub.id.clone())).await; // Send client the lobby id so others can join
    
    // Forward all new messages to lobby, and forward all messages from lobby to client
    loop {
        tokio::select! {
            client_msg = conn.next_msg() =>
                lobby_stub.sender.send((conn.id.clone(), client_msg)).await.unwrap(),
            Some(lobby_msg) = lobby_stub.receiver.recv() =>
                conn.send_msg(lobby_msg).await
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State { lobbies: HashMap::new() };
    let (tx, mut rx) = mpsc::channel::<StateCmd>(32);

    // Spawn state manager
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                StateCmd::CreateLobby { client_id, tx } =>
                    tx.send(state.new_lobby(client_id).await).unwrap(), // Create lobby and send stub
                StateCmd::PushConnection { lobby_id, client_id, tx } =>
                    tx.send(state.register_to_lobby(lobby_id, client_id).await).unwrap(), // Join lobby and send stub
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
