use bincode;
use uuid::Uuid;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::net::{
    TcpListener,
    TcpStream,
};
use tokio_tungstenite::{accept_async, tungstenite::Error};
use tungstenite::{Message, Result};
use yoric_core::*;

#[derive(Debug)]
struct Connection {
    addr: SocketAddr,
    stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>
}

impl Connection {
    fn from_stream(stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>, addr: SocketAddr) -> Connection {
        Connection {
            addr,
            stream
        }
    }

    async fn next(&mut self) -> Message {
        self.stream.next().await.unwrap().unwrap()
    }

    fn send(&mut self, msg: Message) {
        self.stream.send(msg);
    }

    fn send_msg(&mut self, msg: ServerMessage) {
        let encoded: Vec<u8> = bincode::serialize(&msg).unwrap();
        self.send(Message::binary(encoded));
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
                    self.send_msg(ServerMessage::InvalidReq);
                }
            } else {
                self.send_msg(ServerMessage::InvalidReq);
            }
        }
    }

    async fn next_join(&mut self) -> JoinRequest {
        loop {
            let msg = self.next_msg().await;

            match msg {
                ClientMessage::Join(jr) => return jr,
                _ => self.send_msg(ServerMessage::InvalidReq),
            }
        }
    }

    async fn next_do(&mut self) -> Command {
        loop {
            let msg = self.next_msg().await;

            match msg {
                ClientMessage::Do(comm) => return comm,
                _ => self.send_msg(ServerMessage::InvalidReq),
            }
        }
    }
}

fn handle_newlobby(conn: Connection) {
}

fn handle_joinlobby(conn:Connection, id: String) {
}

fn handle_join(req: JoinRequest, conn: Connection) {
    match req {
        JoinRequest::NewLobby => handle_newlobby(conn),
        JoinRequest::JoinLobby(id) => handle_joinlobby(conn, id)
    }
}

async fn handle_connection(peer: SocketAddr, stream: TcpStream) {
    let ws_stream = accept_async(stream).await.unwrap();
    println!("Connection to {} succeeded!", peer);

    let mut conn = Connection::from_stream(ws_stream, peer);

    let jr = conn.next_join().await;
    handle_join(jr, conn);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "192.168.2.22:8000";
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let peer = stream.peer_addr()?;
        println!("Connection from peer address: {}", peer);
        
        tokio::spawn(handle_connection(peer, stream));
    }

    Ok(())
}
