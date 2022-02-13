#[derive(Debug)]
pub struct PartialState { }

use std::collections::BTreeMap;

use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Join(JoinRequest, String),
    Start, // Only allowed if client is host and lobby has 3 players
    Do(Command),
    Chat(String),
    Leave, // Leave lobby
    Dead // Client has disconnected
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    PartialState(GameState),
    InvalidReq,
    LobbyID(String),
    Lobby(LobbyState),
    Chat(String, String),
    Starting
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyState {
    pub count: usize, // Number of players in lobby
    pub unames: Vec<String>,
    pub host: String
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub players: BTreeMap<String, PlayerState>,
    pub phase: Phase,
    pub turn: String,
    pub max_bidder: Option<String>
}

impl GameState {
    pub fn new() -> GameState {
        GameState {
            players: BTreeMap::new(),
            phase: Phase::Registration,
            turn: String::new(),
            max_bidder: None,
        }
    }

    pub fn add_player(&mut self, name: String) -> bool {
        if self.players.insert(name.clone(), PlayerState::new()).is_none() {
            if self.turn.is_empty() {
                self.turn = name;
            }

            return true;
        }

        false
    }

    pub fn partial(&self, name: &String) -> GameState {
        let mut state = self.clone();

        state.players
            .iter_mut()
            .for_each(|(pname, playerstate)| {
                if !name.eq(pname) {
                    playerstate.obscure();
                }
            });

        state
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Phase {
    Registration,
    Play,
    Bid,
    Reveal
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub points: bool,
    pub bid: u8,
    pub cards_in_hand: Cards,
    pub cards_in_play: Vec<Card>,
    pub cards_revealed: Vec<Card>
}

impl PlayerState {
    pub fn new() -> PlayerState {
        PlayerState {
            points: false,
            bid: 0,
            cards_in_hand: Cards {
                skulls: 1,
                roses: 3,
                unknown: 0
            },
            cards_in_play: vec![],
            cards_revealed: vec![]
        }
    }

    pub fn play(&mut self, card: &Card) -> bool {
        if self.cards_in_hand.contains(&card) {
            self.cards_in_hand.decrement(&card);
            self.cards_in_play.push(card.clone());
            true
        } else {
            false
        }
    }

    pub fn reveal(&mut self) -> bool {
        if self.cards_in_play.len() > 0 {
            self.cards_revealed.push(self.cards_in_play.pop().unwrap());
            true
        } else {
            false
        }
    }

    pub fn recall(&mut self) {
        self.cards_in_play
            .drain(0..)
            .for_each(|card| { self.cards_in_hand.increment(&card) });

        self.cards_revealed
            .drain(0..)
            .for_each(|card| { self.cards_in_hand.increment(&card) });
    }

    pub fn obscure(&mut self) {
        self.cards_in_hand.unknown = self.cards_in_hand.roses + self.cards_in_hand.skulls;
        self.cards_in_hand.skulls = 0;
        self.cards_in_hand.roses = 0;

        self.cards_in_play
            .iter_mut()
            .for_each(|card| { *card = Card::Unknown });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cards {
    pub skulls: u8,
    pub roses: u8,
    pub unknown: u8
}

impl Cards {
    pub fn contains(&self, card: &Card) -> bool {
        match card {
            Card::Skull => self.skulls > 0,
            Card::Rose => self.roses > 0,
            Card::Unknown => self.unknown > 0
        }
    }

    pub fn increment(&mut self, card: &Card) {
        match card {
            Card::Skull => self.skulls += 1,
            Card::Rose => self.roses += 1,
            Card::Unknown => self.unknown += 1
        }
    }

    pub fn decrement(&mut self, card: &Card) {
        match card {
            Card::Skull => self.skulls -= 1,
            Card::Rose => self.roses -= 1,
            Card::Unknown => self.unknown -= 1
        }
    }

    pub fn total(&self) -> u8 {
        self.skulls + self.roses + self.unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Card {
    Skull,
    Rose,
    Unknown
}

#[derive(Debug, Serialize, Deserialize)]
pub enum JoinRequest {
    NewLobby,
    JoinLobby(String), // Lobby ID
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    Play(Card),
    Bid(u8),
    Pass,
    Reveal(String) // Player name
}
