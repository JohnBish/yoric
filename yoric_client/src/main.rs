use bincode;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc
};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tungstenite::{Message, Result};
use yoric_core::*;

async fn send_msg(msg: ClientMessage, tx: mpsc::UnboundedSender<Message>) {
    println!("Sending message: {:?}", msg);
    tx.send(Message::binary(bincode::serialize(&msg).unwrap())).unwrap();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connect_addr = "ws://192.168.2.22:8000";

    let url = url::Url::parse(&connect_addr).unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel();

    let tx1 = tx.clone();
    tokio::spawn(async move {
        send_msg(ClientMessage::Join(JoinRequest::NewLobby), tx1.clone()).await;
    });

    let (ws_stream, ws_sink) = connect_async(url).await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");
    let (mut writer, mut reader) = ws_stream.split();
    
    loop {
        tokio::select!{
            msg = rx.recv() => match msg {
                Some(msg) => writer.send(msg).await.unwrap(),
                None => ()
            },
            msg = reader.next() => match msg {
                Some(msg) => match msg {
                    Ok(msg) => {
                        if let Ok(msg) = bincode::deserialize::<ServerMessage>(&msg.into_data()[..]) {
                            println!("Received: {:?}", &msg);
                            send_msg(ClientMessage::Do(Command::Play(Card::Skull)), tx.clone()).await;
                        }
                    }
                    Err(e) => eprintln!("{}", e)
                },
                None => ()
            }
        }
    }
    Ok(())
}
