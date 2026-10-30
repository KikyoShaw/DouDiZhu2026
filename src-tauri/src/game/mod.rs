use std::collections::{HashSet, VecDeque};
use rand::Rng;
use serde::{Deserialize, Serialize};

pub mod cards;
pub mod combo;
pub mod ai;

use cards::{Card, CardId};
use combo::{Combo, ComboKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BidMode { Score, Rob }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage { Lobby, Bidding, Playing, AllPassed, Finished }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatAction {
    pub seat: usize,
    pub kind: String,
    pub text: String,
    pub cards: Vec<Card>,
    pub combo: Option<Combo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultView {
    pub winner: usize,
    pub landlord_won: bool,
    pub spring: Option<String>,
    pub unit_score: i64,
    pub deltas: [i64; 3],
    pub multiplier: u64,
    pub base_score: u32,
}

pub struct Effect { pub kind: &'static str, pub text: String }

// Private hands are never serialized. Session creates one recipient-specific view at a time.
pub struct Round {
    pub(crate) hands: [Vec<CardId>; 3],
    pub(crate) bottom: Vec<CardId>,
    pub stage: Stage,
    pub turn: Option<usize>,
    pub landlord: Option<usize>,
    pub bid_mode: BidMode,
    pub base_score: u32,
    pub highest_bid: u8,
    pub multiplier: u64,
    pub previous: Option<Combo>,
    pub previous_seat: Option<usize>,
    pub table: [Option<SeatAction>; 3],
    pub result: Option<ResultView>,
    pub played: Vec<CardId>,
    play_counts: [u32; 3],
    passes: u8,
    bid_left: u8,
    bidder: Option<usize>,
    first_caller: Option<usize>,
    rob_queue: VecDeque<usize>,
    robbed: bool,
    final_rob: bool,
}

impl Round {
    pub fn lobby(mode: BidMode, base_score: u32) -> Self {
        Self { hands: std::array::from_fn(|_| Vec::new()), bottom: Vec::new(), stage: Stage::Lobby,
            turn: None, landlord: None, bid_mode: mode, base_score, highest_bid: 0, multiplier: 1,
            previous: None, previous_seat: None, table: std::array::from_fn(|_| None), result: None,
            played: Vec::new(), play_counts: [0; 3], passes: 0, bid_left: 3, bidder: None,
            first_caller: None, rob_queue: VecDeque::new(), robbed: false, final_rob: false }
    }
    pub fn deal(mode: BidMode, base_score: u32) -> Self {
        let mut game = Self::lobby(mode, base_score);
        let (hands, bottom) = cards::deal();
        game.hands = hands;
        game.bottom = bottom;
        game.stage = Stage::Bidding;
        game.turn = Some(rand::thread_rng().gen_range(0..3));
        game
    }
    pub fn active(&self) -> bool { matches!(self.stage, Stage::Bidding | Stage::Playing) }
    pub fn can_pass(&self, seat: usize) -> bool {
        self.stage == Stage::Playing && self.turn == Some(seat) && self.previous.is_some()
            && self.previous_seat != Some(seat)
    }
    pub fn bid_prompt(&self) -> String {
        match self.bid_mode {
            BidMode::Score => "叫分".into(),
            BidMode::Rob if self.first_caller.is_some() => "抢地主".into(),
            BidMode::Rob => "叫地主".into(),
        }
    }
    pub fn bid_options(&self) -> Vec<u8> {
        if self.stage != Stage::Bidding { return Vec::new(); }
        match self.bid_mode {
            BidMode::Score => std::iter::once(0).chain(self.highest_bid + 1..=3).collect(),
            BidMode::Rob => vec![0, 1],
        }
    }
    fn check_turn(&self, seat: usize, stage: Stage) -> Result<(), String> {
        if seat >= 3 || self.stage != stage || self.turn != Some(seat) {
            return Err("当前尚未轮到你执行此操作".into());
        }
        Ok(())
    }
    fn assign_landlord(&mut self, seat: usize) {
        self.landlord = Some(seat);
        self.hands[seat].extend(self.bottom.iter().copied());
        cards::sort(&mut self.hands[seat]);
        self.stage = Stage::Playing;
        self.turn = Some(seat);
        self.table = std::array::from_fn(|_| None);
        self.rob_queue.clear();
    }
    fn all_passed(&mut self) {
        self.stage = Stage::AllPassed;
        self.turn = None;
    }
    pub fn bid(&mut self, seat: usize, value: u8) -> Result<Effect, String> {
        self.check_turn(seat, Stage::Bidding)?;
        if !self.bid_options().contains(&value) { return Err("请选择当前有效的叫地主选项".into()); }
        let text = match self.bid_mode {
            BidMode::Score => if value == 0 { "不叫".into() } else { format!("{value} 分") },
            BidMode::Rob if self.first_caller.is_none() => if value == 0 { "不叫".into() } else { "叫地主".into() },
            BidMode::Rob => if value == 0 { "不抢".into() } else { "抢地主".into() },
        };
        self.table[seat] = Some(SeatAction { seat, kind: "bid".into(), text: text.clone(), cards: Vec::new(), combo: None });
        match self.bid_mode {
            BidMode::Score => {
                self.bid_left -= 1;
                if value > 0 {
                    self.highest_bid = value;
                    self.multiplier = u64::from(value);
                    self.bidder = Some(seat);
                }
                if value == 3 || self.bid_left == 0 {
                    if let Some(winner) = self.bidder { self.assign_landlord(winner); }
                    else { self.all_passed(); }
                } else { self.turn = Some((seat + 1) % 3); }
            }
            BidMode::Rob => {
                if self.first_caller.is_none() {
                    self.bid_left -= 1;
                    if value == 1 {
                        self.first_caller = Some(seat);
                        self.bidder = Some(seat);
                        self.highest_bid = 1;
                        self.rob_queue = VecDeque::from([(seat + 1) % 3, (seat + 2) % 3]);
                        self.turn = self.rob_queue.front().copied();
                    } else if self.bid_left == 0 { self.all_passed(); }
                    else { self.turn = Some((seat + 1) % 3); }
                } else {
                    self.rob_queue.pop_front();
                    if value == 1 {
                        self.multiplier = self.multiplier.checked_mul(2).ok_or("倍数超出范围")?;
                        self.bidder = Some(seat);
                        self.robbed = true;
                    }
                    if let Some(next) = self.rob_queue.front() { self.turn = Some(*next); }
                    else if self.robbed && !self.final_rob {
                        self.final_rob = true;
                        let caller = self.first_caller.ok_or("叫地主状态异常")?;
                        self.rob_queue.push_back(caller);
                        self.turn = Some(caller);
                    } else {
                        self.assign_landlord(self.bidder.ok_or("尚未确定地主")?);
                    }
                }
            }
        }
        let suffix = match self.stage {
            Stage::Playing => "，地主确定，开始出牌",
            Stage::AllPassed => "，全员不叫，请重新发牌",
            _ => "",
        };
        Ok(Effect { kind: "bid", text: format!("{text}{suffix}") })
    }
    pub fn play(&mut self, seat: usize, ids: &[CardId]) -> Result<Effect, String> {
        self.check_turn(seat, Stage::Playing)?;
        if ids.is_empty() || ids.len() > 20 { return Err("请选择 1～20 张手牌".into()); }
        let mut seen = HashSet::new();
        if ids.iter().any(|id| *id >= 54 || !seen.insert(*id) || !self.hands[seat].contains(id)) {
            return Err("选牌包含重复牌或不属于你的手牌".into());
        }
        let resolved = combo::resolve(ids, self.previous.as_ref())?;
        let bomb = matches!(resolved.kind, ComboKind::Bomb | ComboKind::Rocket);
        let next_multiplier = if bomb { self.multiplier.checked_mul(2).ok_or("倍数超出范围")? } else { self.multiplier };
        // Compute settlement before mutating the hand, keeping a rejected move atomic.
        let settlement = if self.hands[seat].len() == ids.len() {
            Some(self.settlement(seat, next_multiplier)?)
        } else { None };
        self.hands[seat].retain(|id| !seen.contains(id));
        self.played.extend_from_slice(ids);
        self.play_counts[seat] += 1;
        self.passes = 0;
        self.multiplier = next_multiplier;
        let label = resolved.label.clone();
        self.table[seat] = Some(SeatAction { seat, kind: "play".into(), text: label.clone(),
            cards: resolved.cards.iter().copied().map(cards::card).collect(), combo: Some(resolved.clone()) });
        self.previous = Some(resolved.clone());
        self.previous_seat = Some(seat);
        if let Some(result) = settlement {
            self.multiplier = result.multiplier;
            self.result = Some(result);
            self.stage = Stage::Finished;
            self.turn = None;
        } else {
            self.turn = Some((seat + 1) % 3);
            self.table[(seat + 1) % 3] = None;
        }
        let kind = if self.stage == Stage::Finished { "finish" }
            else if resolved.kind == ComboKind::Rocket { "rocket" }
            else if bomb { "bomb" } else { "play" };
        Ok(Effect { kind, text: label })
    }
    pub fn pass(&mut self, seat: usize) -> Result<Effect, String> {
        self.check_turn(seat, Stage::Playing)?;
        if !self.can_pass(seat) { return Err("你是本轮领出方，必须出牌".into()); }
        self.table[seat] = Some(SeatAction { seat, kind: "pass".into(), text: "不出".into(), cards: Vec::new(), combo: None });
        self.passes += 1;
        if self.passes == 2 {
            self.turn = self.previous_seat;
            self.previous = None;
            self.previous_seat = None;
            self.passes = 0;
            self.table = std::array::from_fn(|_| None);
        } else {
            self.turn = Some((seat + 1) % 3);
            self.table[(seat + 1) % 3] = None;
        }
        Ok(Effect { kind: "pass", text: "不出".into() })
    }
    fn settlement(&self, winner: usize, multiplier: u64) -> Result<ResultView, String> {
        let landlord = self.landlord.ok_or("尚未确定地主")?;
        let landlord_won = winner == landlord;
        let spring = if landlord_won && (0..3).filter(|seat| *seat != landlord).all(|seat| self.play_counts[seat] == 0) {
            Some("春天".to_owned())
        } else if !landlord_won && self.play_counts[landlord] == 1 { Some("反春".to_owned()) }
        else { None };
        let multiplier = if spring.is_some() { multiplier.checked_mul(2).ok_or("倍数超出范围")? } else { multiplier };
        let multiplier_i64 = i64::try_from(multiplier).map_err(|_| "倍数超出范围")?;
        let unit = i64::from(self.base_score).checked_mul(multiplier_i64).ok_or("积分超出范围")?;
        let double = unit.checked_mul(2).ok_or("积分超出范围")?;
        let mut deltas = [if landlord_won { -unit } else { unit }; 3];
        deltas[landlord] = if landlord_won { double } else { -double };
        Ok(ResultView { winner, landlord_won, spring, unit_score: unit, deltas, multiplier, base_score: self.base_score })
    }
    pub fn knowledge(&self, seat: usize) -> ai::Knowledge {
        ai::Knowledge { hand: self.hands[seat].clone(), seat, landlord: self.landlord,
            counts: std::array::from_fn(|index| self.hands[index].len()), previous: self.previous.clone(),
            previous_seat: self.previous_seat, played: self.played.clone() }
    }
}
