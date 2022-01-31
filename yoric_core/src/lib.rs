use uuid::Uuid;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Join(JoinRequest),
    Start,
    Do(Command),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    PartialState,
    InvalidReq,
    LobbyID(String)
}

#[derive(Debug)]
pub struct GameState { }

#[derive(Debug)]
pub struct PartialState { }

#[derive(Debug, Serialize, Deserialize)]
pub enum JoinRequest {
    NewLobby,
    JoinLobby(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Card {
    Skull,
    Rose,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    Play(Card),
    Bid(u8),
}
