use std::{collections::VecDeque, path::PathBuf, sync::{Arc, Mutex, MutexGuard, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, watch};
use crate::{game::{self, ai, cards, BidMode, Effect, ResultView, Round, SeatAction, Stage}, network::{self, PeerRequest, Wire}, storage};
use game::cards::Card;
use game::combo::Combo;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub player_name: String,
    pub bid_mode: BidMode,
    pub base_score: u32,
    pub turn_seconds: u32,
    pub sound: bool,
    pub show_hints: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self { player_name: "我".into(), bid_mode: BidMode::Score, base_score: 1,
            turn_seconds: 20, sound: true, show_hints: true }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        validate_name(&self.player_name)?;
        if !(1..=100).contains(&self.base_score) { return Err("底分应为 1～100".into()); }
        if self.turn_seconds > 120 { return Err("每次操作用时应为 0～120 秒，0 表示不限时".into()); }
        Ok(())
    }
}
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().count() > 24 || name.chars().any(char::is_control) {
        return Err("昵称应为 1～24 个字符，不能包含控制字符".into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GameMode { Single, Lan }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lifecycle { Ready, Running, Paused }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GameAction {
    Start, Pause, Resume, NextRound,
    Bid { value: u8 }, Play { cards: Vec<u8> }, Pass,
    Ready { value: bool }, Autoplay { value: bool },
}
impl GameAction {
    fn idempotent(&self) -> bool { matches!(self, Self::Ready { .. } | Self::Autoplay { .. }) }
    fn turn_action(&self) -> bool { matches!(self, Self::Bid { .. } | Self::Play { .. } | Self::Pass) }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerView {
    pub seat: usize, pub name: String, pub connected: bool, pub bot: bool, pub ready: bool,
    pub autoplay: bool, pub count: usize, pub role: String, pub score: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicEvent { pub id: u64, pub text: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkView { pub status: String, pub role: Option<String>, pub address: String, pub message: String }
impl Default for NetworkView {
    fn default() -> Self { Self { status: "offline".into(), role: None, address: String::new(), message: "离线人机对局".into() } }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionSignal { pub id: u64, pub kind: String, pub seat: Option<usize> }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: u64, pub session_id: u64, pub round_id: u64, pub turn_token: u64,
    pub mode: GameMode, pub lifecycle: Lifecycle, pub stage: Stage, pub local_seat: usize,
    pub players: Vec<PlayerView>, pub my_hand: Vec<Card>, pub bottom: Vec<Card>, pub bottom_hidden: bool,
    pub turn: Option<usize>, pub landlord: Option<usize>, pub bid_mode: BidMode,
    pub bid_options: Vec<u8>, pub bid_prompt: String, pub highest_bid: u8,
    pub previous: Option<Combo>, pub previous_seat: Option<usize>, pub table: [Option<SeatAction>; 3],
    pub multiplier: u64, pub base_score: u32, pub remaining_ms: Option<u64>, pub result: Option<ResultView>,
    pub settings: Settings, pub network: NetworkView, pub events: Vec<PublicEvent>, pub action: ActionSignal,
    pub can_start: bool, pub can_control: bool, pub can_pass: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hint {
    pub revision: u64, pub session_id: u64, pub round_id: u64, pub turn_token: u64,
    pub cards: Vec<u8>, pub pass: bool, pub message: String,
}
struct Peer { id: u64, sender: mpsc::Sender<Wire>, last_request: u64 }
struct Player { name: String, connected: bool, bot: bool, ready: bool, autoplay: bool, score: i64, peer: Option<Peer> }
impl Player {
    fn human(name: String, connected: bool) -> Self {
        Self { name, connected, bot: false, ready: false, autoplay: false, score: 0, peer: None }
    }
    fn bot(name: &str) -> Self {
        Self { name: name.into(), connected: true, bot: true, ready: true, autoplay: false, score: 0, peer: None }
    }
}

struct Session {
    settings: Settings, mode: GameMode, lifecycle: Lifecycle, game: Round,
    players: [Player; 3], revision: u64, session_id: u64, round_id: u64, turn_token: u64,
    deadline: Option<Instant>, frozen: Option<Duration>,
    job_pending: bool, job_cancel: Arc<AtomicBool>, settled_round: Option<u64>,
    network: NetworkView, generation: u64, network_cancel: Option<watch::Sender<bool>>,
    guest_sender: Option<mpsc::Sender<Wire>>, remote: Option<Snapshot>, request_sequence: u64,
    events: VecDeque<PublicEvent>, event_id: u64, action: ActionSignal,
}
impl Session {
    fn new(settings: Settings) -> Self {
        let game = Round::lobby(settings.bid_mode, settings.base_score);
        let players = [Player::human(settings.player_name.clone(), true), Player::bot("阿禾"), Player::bot("小满")];
        Self { settings, mode: GameMode::Single, lifecycle: Lifecycle::Ready, game, players,
            revision: 1, session_id: 1, round_id: 0, turn_token: 0, deadline: None, frozen: None,
            job_pending: false, job_cancel: Arc::new(AtomicBool::new(false)), settled_round: None,
            network: NetworkView::default(), generation: 0, network_cancel: None, guest_sender: None,
            remote: None, request_sequence: 0, events: VecDeque::new(), event_id: 0,
            action: ActionSignal { id: 0, kind: "ready".into(), seat: None } }
    }
    fn guest(&self) -> bool { self.network.role.as_deref() == Some("guest") }
    fn running(&self) -> bool {
        !self.guest() && self.lifecycle == Lifecycle::Running && self.game.active()
            && (self.mode == GameMode::Single || self.players.iter().all(|player| player.connected))
    }
    fn can_start(&self) -> bool {
        !self.guest() && !self.game.active() && (self.mode == GameMode::Single
            || self.players.iter().all(|player| player.connected && player.ready))
    }
    fn cancel_job(&mut self) { self.job_cancel.store(true, Ordering::Relaxed); self.job_pending = false; }
    fn touch(&mut self) { self.revision += 1; }
    fn record(&mut self, text: String) {
        self.event_id += 1;
        self.events.push_back(PublicEvent { id: self.event_id, text });
        while self.events.len() > 30 { self.events.pop_front(); }
    }
    fn signal(&mut self, kind: &str, seat: Option<usize>) {
        self.action.id += 1;
        self.action.kind = kind.into();
        self.action.seat = seat;
    }
    fn remaining_time(&self, now: Instant) -> Option<Duration> {
        self.deadline.map(|deadline| deadline.saturating_duration_since(now)).or(self.frozen)
    }
    fn remaining_ms(&self) -> Option<u64> { self.remaining_time(Instant::now()).map(|time| time.as_millis() as u64) }
    fn freeze(&mut self, now: Instant) { self.frozen = self.remaining_time(now); self.deadline = None; }
    fn resume_clock(&mut self) { self.deadline = self.frozen.take().map(|time| Instant::now() + time); }
    fn reset_clock(&mut self) {
        self.deadline = None;
        self.frozen = if self.game.active() && self.settings.turn_seconds > 0 {
            Some(Duration::from_secs(u64::from(self.settings.turn_seconds)))
        } else { None };
        if self.running() { self.resume_clock(); }
    }
    fn stop_network(&mut self) {
        self.generation += 1;
        if let Some(cancel) = self.network_cancel.take() { let _ = cancel.send(true); }
        self.guest_sender = None;
        for player in &mut self.players { player.peer = None; }
        self.remote = None;
        self.request_sequence = 0;
    }
    fn local_view(&self, seat: usize) -> Snapshot {
        let landlord = self.game.landlord;
        let players = self.players.iter().enumerate().map(|(index, player)| PlayerView {
            seat: index, name: player.name.clone(), connected: player.connected, bot: player.bot,
            ready: player.ready, autoplay: player.autoplay, count: self.game.hands[index].len(),
            role: match landlord { None => "unknown", Some(owner) if owner == index => "landlord", Some(_) => "farmer" }.into(),
            score: player.score,
        }).collect();
        Snapshot { revision: self.revision, session_id: self.session_id, round_id: self.round_id,
            turn_token: self.turn_token, mode: self.mode, lifecycle: self.lifecycle, stage: self.game.stage,
            local_seat: seat, players, my_hand: self.game.hands[seat].iter().copied().map(cards::card).collect(),
            bottom: if landlord.is_some() { self.game.bottom.iter().copied().map(cards::card).collect() } else { Vec::new() },
            bottom_hidden: landlord.is_none(), turn: self.game.turn, landlord, bid_mode: self.game.bid_mode,
            bid_options: self.game.bid_options(), bid_prompt: self.game.bid_prompt(), highest_bid: self.game.highest_bid,
            previous: self.game.previous.clone(), previous_seat: self.game.previous_seat, table: self.game.table.clone(),
            multiplier: self.game.multiplier, base_score: self.game.base_score, remaining_ms: self.remaining_ms(),
            result: self.game.result.clone(), settings: self.settings.clone(), network: self.network.clone(),
            events: self.events.iter().cloned().collect(), action: self.action.clone(),
            can_start: seat == 0 && self.can_start(),
            can_control: seat == 0 && self.game.active() && (self.mode == GameMode::Single || self.network.status == "connected"),
            can_pass: self.lifecycle == Lifecycle::Running && self.game.can_pass(seat) }
    }
    fn snapshot(&self) -> Snapshot {
        if let Some(remote) = self.remote.as_ref().filter(|_| self.guest()) {
            let mut view = remote.clone();
            view.revision = self.revision;
            view.session_id = self.session_id;
            view.lifecycle = self.lifecycle;
            view.settings.player_name = self.settings.player_name.clone();
            view.settings.sound = self.settings.sound;
            view.settings.show_hints = self.settings.show_hints;
            view.network = self.network.clone();
            view.remaining_ms = self.remaining_ms();
            view.can_start = false;
            view.can_control = false;
            view.can_pass = self.network.status == "connected" && self.lifecycle == Lifecycle::Running && remote.can_pass;
            view.action = self.action.clone();
            view
        } else { self.local_view(0) }
    }
    fn check_envelope(&self, revision: u64, session_id: u64, round_id: u64, turn_token: u64, action: &GameAction) -> Result<(), String> {
        if session_id != self.session_id || round_id != self.round_id || revision > self.revision
            || (!action.idempotent() && turn_token != self.turn_token) {
            return Err("牌局已更新，请重新操作".into());
        }
        Ok(())
    }
    fn deal(&mut self) -> Result<(), String> {
        if !self.can_start() { return Err("请等待三位玩家全部连接并准备，且本局结束后再开始".into()); }
        self.cancel_job();
        self.game = Round::deal(self.settings.bid_mode, self.settings.base_score);
        self.lifecycle = Lifecycle::Running;
        self.round_id += 1;
        self.turn_token += 1;
        self.settled_round = None;
        self.signal("deal", None);
        let name = self.players[self.game.turn.unwrap_or(0)].name.clone();
        self.record(format!("第 {} 局发牌完成，由 {name} 先{}", self.round_id, self.game.bid_prompt()));
        self.reset_clock();
        self.touch();
        Ok(())
    }
    fn abort_to_lobby(&mut self, message: &str) {
        self.cancel_job();
        self.game = Round::lobby(self.settings.bid_mode, self.settings.base_score);
        self.lifecycle = Lifecycle::Ready;
        self.round_id += 1;
        self.turn_token += 1;
        self.deadline = None;
        self.frozen = None;
        self.settled_round = None;
        for player in &mut self.players { player.ready = player.bot; player.autoplay = false; }
        self.record(message.into());
        self.touch();
    }
    fn after_turn(&mut self, seat: usize, effect: Effect) {
        self.cancel_job();
        self.turn_token += 1;
        self.signal(effect.kind, Some(seat));
        self.record(format!("{}：{}", self.players[seat].name, effect.text));
        if self.game.stage == Stage::Finished && self.settled_round != Some(self.round_id) {
            if let Some(result) = self.game.result.clone() {
                let updated: Option<Vec<i64>> = self.players.iter().zip(result.deltas).map(|(player, delta)| player.score.checked_add(delta)).collect();
                if let Some(scores) = updated {
                    for (player, score) in self.players.iter_mut().zip(scores) { player.score = score; }
                } else {
                    for (player, delta) in self.players.iter_mut().zip(result.deltas) { player.score = delta; }
                    self.record("累计积分达到上限，已从本局重新计分".into());
                }
                self.record(format!("{}获胜{}，本局单位分 {}", if result.landlord_won { "地主" } else { "农民" },
                    result.spring.as_ref().map(|spring| format!(" · {spring}")).unwrap_or_default(), result.unit_score));
            }
            self.settled_round = Some(self.round_id);
        }
        if !self.game.active() {
            self.lifecycle = Lifecycle::Ready;
            for player in &mut self.players { player.ready = player.bot; }
        }
        self.reset_clock();
        self.touch();
    }
    fn execute_turn(&mut self, seat: usize, action: GameAction) -> Result<(), String> {
        if !self.running() { return Err("对局尚未开始或已暂停".into()); }
        let effect = match action {
            GameAction::Bid { value } => self.game.bid(seat, value)?,
            GameAction::Play { cards } => self.game.play(seat, &cards)?,
            GameAction::Pass => self.game.pass(seat)?,
            _ => return Err("当前操作不是出牌或竞叫操作".into()),
        };
        self.after_turn(seat, effect);
        Ok(())
    }
    fn expire_turn(&mut self, now: Instant) -> bool {
        if !self.running() || !self.deadline.is_some_and(|deadline| now >= deadline) { return false; }
        let Some(seat) = self.game.turn else { return false; };
        let action = if self.game.stage == Stage::Bidding { GameAction::Bid { value: 0 } }
            else if self.game.can_pass(seat) { GameAction::Pass }
            else if let Some(id) = self.game.hands[seat].last() { GameAction::Play { cards: vec![*id] } }
            else { return false; };
        self.record(format!("{} 操作超时，自动{}", self.players[seat].name,
            if self.game.stage == Stage::Bidding { "不叫/不抢" } else if self.game.can_pass(seat) { "不出" } else { "出牌" }));
        self.execute_turn(seat, action).is_ok()
    }
    fn user_action(&mut self, seat: usize, action: GameAction) -> Result<(), String> {
        if seat >= 3 || !self.players[seat].connected { return Err("当前座位未连接".into()); }
        if action.turn_action() {
            if self.players[seat].autoplay { return Err("请先关闭托管再手动操作".into()); }
            return self.execute_turn(seat, action);
        }
        match action {
            GameAction::Start | GameAction::NextRound => {
                if seat != 0 { return Err("请等待房主开始下一局".into()); }
                self.deal()
            }
            GameAction::Pause => {
                if seat != 0 { return Err("暂停由房主控制".into()); }
                if !self.running() { return Err("当前无法暂停".into()); }
                let now = Instant::now();
                if self.expire_turn(now) { return Err("本次操作已超时，当前回合已结束".into()); }
                self.freeze(now);
                self.cancel_job();
                self.lifecycle = Lifecycle::Paused;
                self.turn_token += 1;
                self.record("对局已暂停".into());
                self.touch();
                Ok(())
            }
            GameAction::Resume => {
                if seat != 0 { return Err("继续由房主控制".into()); }
                if self.lifecycle != Lifecycle::Paused || !self.game.active()
                    || (self.mode == GameMode::Lan && !self.players.iter().all(|player| player.connected)) {
                    return Err("当前无法继续对局".into());
                }
                self.lifecycle = Lifecycle::Running;
                self.turn_token += 1;
                self.resume_clock();
                self.record("对局已继续".into());
                self.touch();
                Ok(())
            }
            GameAction::Ready { value } => {
                if self.game.active() { return Err("本局尚未结束".into()); }
                self.players[seat].ready = value;
                self.touch();
                Ok(())
            }
            GameAction::Autoplay { value } => {
                self.players[seat].autoplay = value;
                if self.game.turn == Some(seat) { self.cancel_job(); }
                self.record(format!("{}{}托管", self.players[seat].name, if value { "开启" } else { "关闭" }));
                self.touch();
                Ok(())
            }
            _ => Err("当前操作无效".into()),
        }
    }
}

struct Job { knowledge: ai::Knowledge, stage: Stage, bid_mode: BidMode, highest_bid: u8, robbing: bool,
    session_id: u64, round_id: u64, turn_token: u64, cancel: Arc<AtomicBool> }
pub struct Hub { inner: Mutex<Session>, app: AppHandle, directory: PathBuf }
impl Hub {
    pub fn new(app: AppHandle, directory: PathBuf) -> Arc<Self> {
        let settings = storage::load(&directory);
        Arc::new(Self { inner: Mutex::new(Session::new(settings)), app, directory })
    }
    fn lock(&self) -> MutexGuard<'_, Session> { self.inner.lock().unwrap_or_else(|error| error.into_inner()) }
    pub fn snapshot(&self) -> Snapshot { self.lock().snapshot() }
    pub fn shutdown(&self) { let mut state = self.lock(); state.cancel_job(); state.stop_network(); }
    pub fn refresh(self: &Arc<Self>) -> Snapshot {
        let (view, peers, generation, job) = {
            let mut state = self.lock();
            let job = if state.running() && !state.job_pending {
                state.game.turn.filter(|seat| state.players[*seat].bot || state.players[*seat].autoplay).map(|seat| {
                    state.job_pending = true;
                    state.job_cancel = Arc::new(AtomicBool::new(false));
                    Job { knowledge: state.game.knowledge(seat), stage: state.game.stage, bid_mode: state.game.bid_mode,
                        highest_bid: state.game.highest_bid, robbing: state.game.bid_prompt() == "抢地主",
                        session_id: state.session_id, round_id: state.round_id, turn_token: state.turn_token,
                        cancel: state.job_cancel.clone() }
                })
            } else { None };
            let peers: Vec<_> = if state.guest() { Vec::new() } else {
                state.players.iter().enumerate().filter_map(|(seat, player)| player.peer.as_ref()
                    .map(|peer| (peer.id, peer.sender.clone(), state.local_view(seat)))).collect()
            };
            (state.snapshot(), peers, state.generation, job)
        };
        let _ = self.app.emit("game:snapshot", &view);
        for (id, sender, snapshot) in peers {
            if sender.try_send(Wire::State { snapshot: Box::new(snapshot) }).is_err() {
                self.host_peer_lost(generation, id, "发送队列不可用，连接已断开".into());
            }
        }
        if let Some(job) = job {
            let hub = self.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(650)).await;
                if job.cancel.load(Ordering::Relaxed) { return; }
                let seat = job.knowledge.seat;
                let flag = job.cancel.clone();
                let cancel = job.cancel.clone();
                let session_id = job.session_id;
                let round_id = job.round_id;
                let turn_token = job.turn_token;
                let decision = tauri::async_runtime::spawn_blocking(move || {
                    if flag.load(Ordering::Relaxed) { return None; }
                    Some(if job.stage == Stage::Bidding {
                        let value = match job.bid_mode {
                            BidMode::Score => ai::bid_score(&job.knowledge.hand, job.highest_bid),
                            BidMode::Rob => u8::from(ai::want_landlord(&job.knowledge.hand, job.robbing)),
                        };
                        GameAction::Bid { value }
                    } else {
                        ai::suggest(&job.knowledge).map(|combo| GameAction::Play { cards: combo.cards }).unwrap_or(GameAction::Pass)
                    })
                }).await;
                let changed = {
                    let mut state = hub.lock();
                    if cancel.load(Ordering::Relaxed) || state.session_id != session_id || state.round_id != round_id
                        || state.turn_token != turn_token || !state.running() || state.game.turn != Some(seat)
                        || !(state.players[seat].bot || state.players[seat].autoplay) { false }
                    else if state.expire_turn(Instant::now()) { true }
                    else {
                        state.job_pending = false;
                        let action = decision.ok().flatten().unwrap_or_else(|| fallback_action(&state.game, seat));
                        if state.execute_turn(seat, action).is_err() {
                            let fallback = fallback_action(&state.game, seat);
                            let _ = state.execute_turn(seat, fallback);
                        }
                        true
                    }
                };
                if changed { hub.refresh(); }
            });
        }
        view
    }
    pub fn start_clock(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                let Some(hub) = weak.upgrade() else { break; };
                let changed = hub.lock().expire_turn(Instant::now());
                if changed { hub.refresh(); }
            }
        });
    }
    pub fn new_match(self: &Arc<Self>, mut settings: Settings) -> Result<Snapshot, String> {
        settings.player_name = settings.player_name.trim().into();
        settings.validate()?;
        {
            let mut state = self.lock();
            if state.mode == GameMode::Lan { return Err("请先离开网络房间再修改对局规则".into()); }
            storage::save(&self.directory, &settings)?;
            let revision = state.revision;
            let session_id = state.session_id + 1;
            let round_id = state.round_id + 1;
            let generation = state.generation;
            let signal_id = state.action.id;
            state.cancel_job();
            *state = Session::new(settings);
            state.revision = revision + 1;
            state.session_id = session_id;
            state.round_id = round_id;
            state.generation = generation;
            state.action.id = signal_id;
            state.record("新对局已准备，点击开始发牌".into());
        }
        Ok(self.refresh())
    }
    pub fn preferences(self: &Arc<Self>, sound: bool, show_hints: bool) -> Result<Snapshot, String> {
        {
            let mut state = self.lock();
            let mut settings = state.settings.clone();
            settings.sound = sound;
            settings.show_hints = show_hints;
            storage::save(&self.directory, &settings)?;
            state.settings = settings;
            state.touch();
        }
        Ok(self.refresh())
    }
    pub fn action(self: &Arc<Self>, action: GameAction, revision: u64, session_id: u64, round_id: u64, turn_token: u64) -> Result<Snapshot, String> {
        let result = {
            let mut state = self.lock();
            if state.guest() {
                let view = state.snapshot();
                if state.network.status != "connected" { return Err("连接尚未建立或已断开".into()); }
                if view.session_id != session_id || view.round_id != round_id || (!action.idempotent() && view.turn_token != turn_token) {
                    return Err("牌局已更新，请重新操作".into());
                }
                if matches!(action, GameAction::Start | GameAction::NextRound | GameAction::Pause | GameAction::Resume) {
                    return Err("开局、暂停和继续由房主控制".into());
                }
                let remote = state.remote.as_ref().ok_or("请等待房主同步房间")?;
                let mut request = PeerRequest { request_id: 0, revision: remote.revision, session_id: remote.session_id,
                    round_id: remote.round_id, turn_token: remote.turn_token, action };
                state.request_sequence += 1;
                request.request_id = state.request_sequence;
                let queued = state.guest_sender.as_ref().is_some_and(|sender| sender.try_send(Wire::Action { request }).is_ok());
                if !queued {
                    let token = state.generation;
                    drop(state);
                    self.guest_lost(token, "连接发送队列不可用，请离开房间后重新加入".into());
                    return Err("连接已断开，请重新加入房间".into());
                }
                return Ok(view);
            }
            state.check_envelope(revision, session_id, round_id, turn_token, &action)?;
            if state.expire_turn(Instant::now()) && !action.idempotent() { Err("本次操作已超时，牌局已经更新".into()) }
            else { state.user_action(0, action) }
        };
        let view = self.refresh();
        result.map(|_| view)
    }
    pub async fn hint(self: &Arc<Self>, revision: u64, session_id: u64, round_id: u64, turn_token: u64) -> Result<Hint, String> {
        let knowledge = {
            let state = self.lock();
            let view = state.snapshot();
            if view.revision != revision || view.session_id != session_id || view.round_id != round_id || view.turn_token != turn_token {
                return Err("牌局已更新，请重新获取提示".into());
            }
            if view.lifecycle != Lifecycle::Running || view.stage != Stage::Playing || view.turn != Some(view.local_seat)
                || view.players[view.local_seat].autoplay || (view.mode == GameMode::Lan && view.network.status != "connected") {
                return Err("现在不能获取出牌提示".into());
            }
            if state.guest() {
                ai::Knowledge { hand: view.my_hand.iter().map(|card| card.id).collect(), seat: view.local_seat,
                    landlord: view.landlord, counts: std::array::from_fn(|seat| view.players[seat].count),
                    previous: view.previous, previous_seat: view.previous_seat,
                    played: Vec::new() }
            } else { state.game.knowledge(0) }
        };
        let suggestion = tauri::async_runtime::spawn_blocking(move || ai::suggest(&knowledge)).await
            .map_err(|error| format!("提示暂不可用：{error}"))?;
        let now = self.snapshot();
        if now.revision != revision || now.session_id != session_id || now.round_id != round_id || now.turn_token != turn_token {
            return Err("牌局已更新，请重新获取提示".into());
        }
        Ok(Hint { revision, session_id, round_id, turn_token, pass: suggestion.is_none(),
            message: suggestion.as_ref().map(|combo| format!("建议出 {}", combo.label)).unwrap_or("建议不出，保留手牌".into()),
            cards: suggestion.map(|combo| combo.cards).unwrap_or_default() })
    }
    pub fn begin_host(self: &Arc<Self>, listener: tokio::net::TcpListener, mut settings: Settings, port: u16) -> Result<Snapshot, String> {
        settings.player_name = settings.player_name.trim().into();
        settings.validate()?;
        let (token, receiver) = {
            let mut state = self.lock();
            if state.mode == GameMode::Lan { return Err("请先离开当前房间".into()); }
            storage::save(&self.directory, &settings)?;
            state.cancel_job();
            state.stop_network();
            state.settings = settings;
            state.mode = GameMode::Lan;
            state.session_id += 1;
            state.players = [Player::human(state.settings.player_name.clone(), true), Player::human("等待玩家".into(), false), Player::human("等待玩家".into(), false)];
            state.network = NetworkView { status: "hosting".into(), role: Some("host".into()), address: format!("0.0.0.0:{port}"),
                message: format!("房间已创建，等待两名玩家；请告知对方本机局域网 IP 和端口 {port}") };
            state.abort_to_lobby("房间已创建，三位玩家准备后由房主开始");
            let (sender, receiver) = watch::channel(false);
            state.network_cancel = Some(sender);
            (state.generation, receiver)
        };
        network::host(self.clone(), listener, token, receiver);
        Ok(self.refresh())
    }
    pub fn begin_join(self: &Arc<Self>, address: String, port: u16, name: String) -> Result<Snapshot, String> {
        validate_name(&name)?;
        if address.trim().is_empty() || address.len() > 253 || port == 0 { return Err("请输入有效的主机地址和端口".into()); }
        let (token, receiver) = {
            let mut state = self.lock();
            if state.mode == GameMode::Lan { return Err("请先离开当前房间".into()); }
            let mut settings = state.settings.clone();
            settings.player_name = name.trim().into();
            storage::save(&self.directory, &settings)?;
            state.cancel_job();
            state.stop_network();
            state.settings = settings;
            state.mode = GameMode::Lan;
            state.session_id += 1;
            state.abort_to_lobby("正在连接房间");
            state.players = [Player::human(state.settings.player_name.clone(), true), Player::human("等待玩家".into(), false), Player::human("等待玩家".into(), false)];
            state.network = NetworkView { status: "connecting".into(), role: Some("guest".into()), address: format!("{}:{port}", address.trim()), message: "正在连接房主…".into() };
            let (sender, receiver) = watch::channel(false);
            state.network_cancel = Some(sender);
            (state.generation, receiver)
        };
        network::join(self.clone(), address.trim().into(), port, name.trim().into(), token, receiver);
        Ok(self.refresh())
    }
    pub fn leave(self: &Arc<Self>) -> Result<Snapshot, String> {
        let settings = self.lock().settings.clone();
        {
            let mut state = self.lock();
            state.cancel_job();
            state.stop_network();
            state.mode = GameMode::Single;
            state.lifecycle = Lifecycle::Ready;
            state.session_id += 1;
            state.players = [Player::human(settings.player_name.clone(), true), Player::bot("阿禾"), Player::bot("小满")];
            state.network = NetworkView::default();
            state.abort_to_lobby("已离开房间，点击开始进行离线人机对局");
        }
        Ok(self.refresh())
    }
    pub fn network_is_current(&self, token: u64) -> bool { self.lock().generation == token }
    pub fn host_peer_connected(self: &Arc<Self>, token: u64, peer_id: u64, name: String, _address: String, sender: mpsc::Sender<Wire>) -> Result<(), String> {
        validate_name(&name)?;
        {
            let mut state = self.lock();
            if state.generation != token || state.network.role.as_deref() != Some("host") { return Err("房间已经关闭".into()); }
            if state.game.active() { return Err("房间正在对局，请稍后加入".into()); }
            let seat = (1..3).find(|seat| !state.players[*seat].connected).ok_or("房间已满，请加入其他房间")?;
            state.players[seat] = Player::human(name.trim().into(), true);
            state.players[seat].peer = Some(Peer { id: peer_id, sender, last_request: 0 });
            state.network.status = if state.players.iter().all(|player| player.connected) { "connected" } else { "hosting" }.into();
            state.network.message = format!("{} 已加入，请各位玩家准备", name.trim());
            // A replacement player starts a fresh table score instead of inheriting a previous occupant's score.
            for player in &mut state.players { player.score = 0; player.ready = false; }
            state.abort_to_lobby("房间成员已更新，累计积分归零，请重新准备");
        }
        self.refresh();
        Ok(())
    }
    pub fn host_peer_lost(self: &Arc<Self>, token: u64, peer_id: u64, message: String) {
        {
            let mut state = self.lock();
            if state.generation != token || state.guest() { return; }
            let Some(seat) = state.players.iter().position(|player| player.peer.as_ref().is_some_and(|peer| peer.id == peer_id)) else { return; };
            let name = state.players[seat].name.clone();
            state.players[seat] = Player::human("等待玩家".into(), false);
            state.network.status = "hosting".into();
            state.network.message = format!("{name} 已离开：{message}");
            let text = if state.game.active() { "玩家断线，本局中止且不结算积分；等待新玩家后重新准备" } else { "玩家已离开，等待新玩家后重新准备" };
            state.abort_to_lobby(text);
        }
        self.refresh();
    }
    pub fn receive_peer(self: &Arc<Self>, token: u64, peer_id: u64, request: PeerRequest) {
        let (error, sender) = {
            let mut state = self.lock();
            if state.generation != token || state.guest() { return; }
            let Some(seat) = state.players.iter().position(|player| player.peer.as_ref().is_some_and(|peer| peer.id == peer_id)) else { return; };
            let Some(peer) = state.players[seat].peer.as_mut() else { return; };
            let sender = peer.sender.clone();
            let duplicate = request.request_id <= peer.last_request;
            peer.last_request = peer.last_request.max(request.request_id);
            let error = if duplicate { None }
            else if matches!(request.action, GameAction::Start | GameAction::NextRound | GameAction::Pause | GameAction::Resume) {
                Some("开局、暂停和继续由房主控制".into())
            } else {
                let validation = state.check_envelope(request.revision, request.session_id, request.round_id, request.turn_token, &request.action);
                match validation {
                    Err(error) => Some(error),
                    Ok(()) => {
                        if state.expire_turn(Instant::now()) && !request.action.idempotent() { Some("本次操作已超时".into()) }
                        else { state.user_action(seat, request.action).err() }
                    }
                }
            };
            (error, sender)
        };
        if let Some(message) = error {
            if sender.try_send(Wire::Error { message }).is_err() { self.host_peer_lost(token, peer_id, "连接不可用".into()); return; }
        }
        self.refresh();
    }
    pub fn guest_connected(self: &Arc<Self>, token: u64, sender: mpsc::Sender<Wire>, address: String) -> bool {
        {
            let mut state = self.lock();
            if state.generation != token || !state.guest() { return false; }
            state.guest_sender = Some(sender);
            state.network.status = "connected".into();
            state.network.address = address;
            state.network.message = "已连接，等待房主同步座位".into();
            state.touch();
        }
        self.refresh();
        true
    }
    pub fn receive_snapshot(self: &Arc<Self>, token: u64, snapshot: Snapshot) -> Result<(), String> {
        {
            let mut state = self.lock();
            if state.generation != token || !state.guest() { return Ok(()); }
            if state.remote.as_ref().is_some_and(|remote| snapshot.revision <= remote.revision) { return Ok(()); }
            validate_snapshot(&snapshot)?;
            state.settings.bid_mode = snapshot.settings.bid_mode;
            state.settings.base_score = snapshot.settings.base_score;
            state.settings.turn_seconds = snapshot.settings.turn_seconds;
            state.lifecycle = snapshot.lifecycle;
            state.network.message = snapshot.network.message.clone();
            state.frozen = snapshot.remaining_ms.map(Duration::from_millis);
            state.deadline = if snapshot.lifecycle == Lifecycle::Running && matches!(snapshot.stage, Stage::Bidding | Stage::Playing) {
                state.frozen.take().map(|remaining| Instant::now() + remaining)
            } else { None };
            if state.remote.as_ref().is_some_and(|old| old.action.id != snapshot.action.id) {
                state.signal(&snapshot.action.kind, snapshot.action.seat);
            }
            state.remote = Some(snapshot);
            state.touch();
        }
        self.refresh();
        Ok(())
    }
    pub fn guest_lost(self: &Arc<Self>, token: u64, message: String) {
        {
            let mut state = self.lock();
            if state.generation != token || !state.guest() || state.network.status == "disconnected" { return; }
            state.freeze(Instant::now());
            state.cancel_job();
            if state.lifecycle == Lifecycle::Running { state.lifecycle = Lifecycle::Paused; }
            state.guest_sender = None;
            if let Some(cancel) = state.network_cancel.take() { let _ = cancel.send(true); }
            state.generation += 1;
            state.network.status = "disconnected".into();
            state.network.message = format!("连接已断开，本局不再继续：{message}");
            state.touch();
        }
        let _ = self.app.emit("game:snapshot", self.snapshot());
    }
    pub fn network_notice(self: &Arc<Self>, token: u64, message: String) {
        {
            let mut state = self.lock();
            if state.generation != token { return; }
            state.network.message = message;
            state.touch();
        }
        self.refresh();
    }
}

fn fallback_action(game: &Round, seat: usize) -> GameAction {
    if game.stage == Stage::Bidding { GameAction::Bid { value: 0 } }
    else if game.can_pass(seat) { GameAction::Pass }
    else { GameAction::Play { cards: game.hands[seat].last().copied().into_iter().collect() } }
}
fn validate_snapshot(view: &Snapshot) -> Result<(), String> {
    view.settings.validate()?;
    if view.mode != GameMode::Lan || !(1..=2).contains(&view.local_seat) || view.players.len() != 3
        || view.players.iter().enumerate().any(|(seat, player)| player.seat != seat || player.count > 20 || validate_name(&player.name).is_err())
        || view.my_hand.len() > 20 || view.turn.is_some_and(|seat| seat >= 3) || view.landlord.is_some_and(|seat| seat >= 3)
        || view.previous_seat.is_some_and(|seat| seat >= 3) || view.events.len() > 30 || view.bottom.len() > 3 {
        return Err("收到无效的房间状态".into());
    }
    let mut ids = std::collections::HashSet::new();
    if view.my_hand.iter().any(|card| card.id >= 54 || cards::card(card.id) != *card || !ids.insert(card.id))
        || view.my_hand.len() != view.players[view.local_seat].count
        || (view.bottom_hidden && !view.bottom.is_empty())
        || view.bottom.iter().any(|card| card.id >= 54 || cards::card(card.id) != *card)
        || view.table.iter().flatten().any(|action| action.seat >= 3 || action.cards.len() > 20
            || action.cards.iter().any(|card| card.id >= 54 || cards::card(card.id) != *card)) {
        return Err("收到无效的手牌或出牌内容".into());
    }
    if view.remaining_ms.is_some_and(|time| time > 120_000) { return Err("收到无效的计时状态".into()); }
    Ok(())
}
