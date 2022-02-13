use std::collections::BTreeMap;

use uuid::Uuid;
use yoric_core::{GameState, Command, ServerMessage, Phase, Card, PlayerState};

use crate::ClientHandle;

pub async fn handle_play(card: Card, player: &mut PlayerState) -> bool {
    player.play(&card)
}

pub async fn handle_bid(bid: u8, state: &GameState) -> bool {
    let mut in_play = 0;
    let highest: u8;
    for player in state.players.iter() {
        in_play += player.1.cards_in_play.len();
    }
    if let Some(player) = &state.max_bidder {
        highest = state.players.get(player).unwrap().bid;
    } else {
        highest = 0;
    }

    (bid > highest) && (in_play >= 1) && (bid <= in_play as u8)
}

pub async fn handle_reveal(player: String) -> bool {
    false
}

pub fn next_turn(state: &mut GameState) {
    state.turn = state.players
        .range((state.turn.clone())..)
        .nth(1) // Range is inclusive, so nth(0) is state.turn
        .unwrap_or_else(|| state.players.first_key_value().unwrap())
        .0
        .clone();
    println!("Turn over. {}'s turn begins", state.turn);
}

pub async fn handle_action(cmd: Command, id: Uuid, state: &mut GameState, connections: &BTreeMap<Uuid, ClientHandle>) -> (bool, Option<String>) {
    let caller_handle = connections.get(&id).unwrap();

    if !caller_handle.uname.eq(&state.turn) { // Ignore if it's not the caller's turn
        println!("Received command from {}. Rejecting since it is {}'s turn", &caller_handle.uname, &state.turn);
        caller_handle.sender.send(ServerMessage::InvalidReq).await.unwrap();
        return (false, None);
    }

    let uname = &caller_handle.uname;
    let player_state = state.players.get_mut(uname).unwrap();

    match cmd {
        Command::Play(card) => {
            if matches!(state.phase, Phase::Play) {
                if handle_play(card, player_state).await {
                    next_turn(state);
                    (true, Some(format!("{}'s turn!", &state.turn)))
                } else {
                    (false, None)
                }

            } else {
                (false, None)
            }
        },
        Command::Bid(bid) => {
            if matches!(state.phase, Phase::Play | Phase::Bid) {
                if handle_bid(bid, state).await {
                    state.phase = Phase::Bid;
                    state.max_bidder = Some(uname.clone());

                    next_turn(state);
                    (true, Some(format!("{} bid {}. {}'s turn!", &uname, bid, &state.turn)))
                } else {
                    (false, None)
                }
            } else {
                (false, None)
            }
        },
        Command::Pass => {
            if matches!(state.phase, Phase::Bid) {
                next_turn(state);
                if let Some(bidder) = &state.max_bidder {
                    if state.turn.eq(bidder) {
                        state.phase = Phase::Reveal;
                        return (true, Some(format!("{} won the bid", &state.turn)));
                    }
                }

                (true, Some(format!("{}'s turn!", &state.turn)))
            } else {
                (false, None)
            }
        },
        Command::Reveal(player) => {
            if matches!(state.phase, Phase::Reveal) {
                (handle_reveal(player).await, None)
            } else {
                (false, None)
            }
        }
    }
}
