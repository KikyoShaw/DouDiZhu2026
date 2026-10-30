use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

pub type CardId = u8;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Suit {
    Spade,
    Heart,
    Club,
    Diamond,
    Joker,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: u8,
    pub rank: u8,
    pub suit: Suit,
}

pub fn rank(id: u8) -> u8 {
    match id {
        0..=51 => id / 4 + 3,
        52 => 16,
        53 => 17,
        _ => 0,
    }
}

pub fn card(id: u8) -> Card {
    let suit = if id >= 52 {
        Suit::Joker
    } else {
        match id % 4 {
            0 => Suit::Spade,
            1 => Suit::Heart,
            2 => Suit::Club,
            _ => Suit::Diamond,
        }
    };
    Card {
        id,
        rank: rank(id),
        suit,
    }
}

pub fn sort(hand: &mut Vec<u8>) {
    hand.sort_unstable_by(|a, b| rank(*b).cmp(&rank(*a)).then_with(|| b.cmp(a)));
}

pub fn deal() -> ([Vec<u8>; 3], Vec<u8>) {
    let mut deck: Vec<u8> = (0..54).collect();
    deck.shuffle(&mut rand::thread_rng());
    let mut hands = [
        deck[..17].to_vec(),
        deck[17..34].to_vec(),
        deck[34..51].to_vec(),
    ];
    let mut bottom = deck[51..].to_vec();
    for hand in &mut hands {
        sort(hand);
    }
    sort(&mut bottom);
    (hands, bottom)
}