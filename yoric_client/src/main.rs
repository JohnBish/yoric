use bincode;
use futures_util::{future, pin_mut, StreamExt};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc
};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use yoric_core::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connect_addr = "ws://192.168.2.22:8000";

    let url = url::Url::parse(&connect_addr).unwrap();

    let (tx, rx) = futures_channel::mpsc::unbounded();
    // tokio::spawn(read_stdin(stdin_tx));
    tokio::spawn(async move {
        let serialized = bincode::serialize(&ClientMessage::Join(JoinRequest::NewLobby)).unwrap();
        println!("{:?}", serialized);
        let deserialized: ClientMessage = bincode::deserialize(&serialized[..]).unwrap();
        println!("{:?}", deserialized);
        tx.unbounded_send(Message::binary(bincode::serialize(&ClientMessage::Join(JoinRequest::NewLobby)).unwrap()))
    });

    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");

    let (write, read) = ws_stream.split();

    let stdin_to_ws = rx.map(Ok).forward(write);
    let ws_to_stdout = {
        read.for_each(|message| async {
            let data = message.unwrap().into_data();
            tokio::io::stdout().write_all(&data).await.unwrap();
        })
    };

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;

    Ok(())
}
