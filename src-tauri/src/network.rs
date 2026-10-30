use std::{
    future::{poll_fn, Future},
    pin::{pin, Pin},
    sync::Arc,
    task::Poll,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
    net::TcpStream,
    sync::{mpsc, oneshot, watch},
    time::{self, Instant, MissedTickBehavior},
};

use crate::session::{GameAction, Hub, Snapshot};

pub const PROTOCOL: u16 = 1;
pub const PRODUCT: &str = "doudizhu2026";

const QUEUE_CAPACITY: usize = 128;
const MAX_FRAME_BYTES: usize = 65_536;
const MAX_HELLO_BYTES: usize = 256;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const FRAME_TIMEOUT: Duration = Duration::from_secs(35);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Wire {
    Hello {
        product: String,
        protocol: u16,
        name: String,
    },
    State {
        snapshot: Box<Snapshot>,
    },
    Action {
        request: PeerRequest,
    },
    Error {
        message: String,
    },
    Ping,
    Pong,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerRequest {
    pub request_id: u64,
    pub revision: u64,
    pub session_id: u64,
    pub round_id: u64,
    pub turn_token: u64,
    pub action: GameAction,
}

#[derive(Clone, Copy)]
enum Role {
    Host { peer_id: u64 },
    Guest,
}

enum ReadExit {
    Stopped,
    Lost(String),
    Reject(String),
}

enum WriteEvent {
    Frame(Wire),
    Reject(String),
    Closed,
    Full,
}

pub fn host(
    hub: Arc<Hub>,
    listener: TcpListener,
    token: u64,
    mut cancel: watch::Receiver<bool>,
) {
    tauri::async_runtime::spawn(async move {
        let mut next_peer_id = 1_u64;
        loop {
            if !is_current(&hub, token, &cancel) {
                return;
            }

            let (stream, address) = match until_cancelled(&mut cancel, listener.accept()).await {
                None => return,
                Some(Ok(connection)) => connection,
                Some(Err(error)) => {
                    if is_current(&hub, token, &cancel) {
                        hub.network_notice(token, format!("局域网监听已停止：{error}"));
                    }
                    return;
                }
            };

            if !is_current(&hub, token, &cancel) {
                return;
            }
            let peer_id = next_peer_id;
            next_peer_id = match next_peer_id.checked_add(1) {
                Some(next) => next,
                None => {
                    if is_current(&hub, token, &cancel) {
                        hub.network_notice(token, "连接编号已耗尽，请重新创建房间".to_owned());
                    }
                    return;
                }
            };

            let peer_hub = Arc::clone(&hub);
            let peer_cancel = cancel.clone();
            // 握手独立执行；席位是否可用始终由 Hub 判定，监听不随人数暂停。
            tauri::async_runtime::spawn(async move {
                accept_peer(
                    peer_hub,
                    stream,
                    address.to_string(),
                    token,
                    peer_id,
                    peer_cancel,
                )
                .await;
            });
        }
    });
}

pub fn join(
    hub: Arc<Hub>,
    address: String,
    port: u16,
    name: String,
    token: u64,
    mut cancel: watch::Receiver<bool>,
) {
    tauri::async_runtime::spawn(async move {
        if !is_current(&hub, token, &cancel) {
            return;
        }
        let name = match validate_name(&name) {
            Ok(name) => name,
            Err(message) => {
                if is_current(&hub, token, &cancel) {
                    hub.guest_lost(token, message);
                }
                return;
            }
        };
        let address = address.trim().to_owned();
        let connection = time::timeout(
            HANDSHAKE_TIMEOUT,
            TcpStream::connect((address.as_str(), port)),
        );
        let mut stream = match until_cancelled(&mut cancel, connection).await {
            None => return,
            Some(Ok(Ok(stream))) => stream,
            Some(result) => {
                let message = match result {
                    Ok(Err(error)) => format!("连接房主失败：{error}"),
                    Err(_) => "连接房主超时".to_owned(),
                    Ok(Ok(_)) => unreachable!(),
                };
                if is_current(&hub, token, &cancel) {
                    hub.guest_lost(token, message);
                }
                return;
            }
        };
        if !is_current(&hub, token, &cancel) {
            return;
        }
        let _ = stream.set_nodelay(true);

        match until_cancelled(&mut cancel, exchange_hello(&mut stream, name)).await {
            None => return,
            Some(Ok(_)) => {}
            Some(Err(message)) => {
                if is_current(&hub, token, &cancel) {
                    let _ = until_cancelled(&mut cancel, send_rejection(&mut stream, &message)).await;
                }
                if is_current(&hub, token, &cancel) {
                    hub.guest_lost(token, message);
                }
                return;
            }
        }
        if !is_current(&hub, token, &cancel) {
            return;
        }

        let remote_address = stream
            .peer_addr()
            .map(|peer| peer.to_string())
            .unwrap_or_else(|_| format!("{address}:{port}"));
        let (sender, receiver) = mpsc::channel(QUEUE_CAPACITY);
        let replies = sender.downgrade();
        if !hub.guest_connected(token, sender, remote_address) {
            return;
        }

        if let Some(message) =
            connection_loop(&hub, stream, token, Role::Guest, receiver, replies, &mut cancel).await
        {
            if is_current(&hub, token, &cancel) {
                hub.guest_lost(token, message);
            }
        }
    });
}

async fn accept_peer(
    hub: Arc<Hub>,
    mut stream: TcpStream,
    address: String,
    token: u64,
    peer_id: u64,
    mut cancel: watch::Receiver<bool>,
) {
    if !is_current(&hub, token, &cancel) {
        return;
    }
    let _ = stream.set_nodelay(true);
    let name = match until_cancelled(
        &mut cancel,
        exchange_hello(&mut stream, "房主".to_owned()),
    )
    .await
    {
        None => return,
        Some(Ok(name)) => name,
        Some(Err(message)) => {
            if is_current(&hub, token, &cancel) {
                let _ = until_cancelled(&mut cancel, send_rejection(&mut stream, &message)).await;
            }
            return;
        }
    };
    if !is_current(&hub, token, &cancel) {
        return;
    }

    let (sender, receiver) = mpsc::channel(QUEUE_CAPACITY);
    let replies = sender.downgrade();
    // Hub 先分配固定席位并排入初始快照，随后写任务才开始消费队列。
    if let Err(message) = hub.host_peer_connected(token, peer_id, name, address, sender) {
        if is_current(&hub, token, &cancel) {
            let _ = until_cancelled(&mut cancel, send_rejection(&mut stream, &message)).await;
        }
        return;
    }

    if let Some(message) = connection_loop(
        &hub,
        stream,
        token,
        Role::Host { peer_id },
        receiver,
        replies,
        &mut cancel,
    )
    .await
    {
        if is_current(&hub, token, &cancel) {
            hub.host_peer_lost(token, peer_id, message);
        }
    }
}

async fn exchange_hello(stream: &mut TcpStream, name: String) -> Result<String, String> {
    time::timeout(HANDSHAKE_TIMEOUT, async {
        let hello = Wire::Hello {
            product: PRODUCT.to_owned(),
            protocol: PROTOCOL,
            name,
        };
        write_frame(stream, &hello, MAX_HELLO_BYTES).await?;
        match read_frame(stream, MAX_HELLO_BYTES).await? {
            Wire::Hello {
                product,
                protocol,
                name,
            } => {
                if product != PRODUCT {
                    return Err("对方不是斗地主客户端".to_owned());
                }
                if protocol != PROTOCOL {
                    return Err("客户端协议版本不一致".to_owned());
                }
                validate_name(&name)
            }
            Wire::Error { message } => Err(message),
            _ => Err("连接的首条消息必须是 hello".to_owned()),
        }
    })
    .await
    .map_err(|_| "连接握手超时".to_owned())?
}

fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if !(1..=24).contains(&name.chars().count()) {
        return Err("玩家名称须为 1 至 24 个字符".to_owned());
    }
    Ok(name.to_owned())
}

async fn send_rejection(stream: &mut TcpStream, message: &str) -> Result<(), String> {
    let error = Wire::Error {
        message: message.to_owned(),
    };
    time::timeout(
        HANDSHAKE_TIMEOUT,
        write_frame(stream, &error, MAX_FRAME_BYTES),
    )
    .await
    .map_err(|_| "发送拒绝消息超时".to_owned())?
}

async fn connection_loop(
    hub: &Arc<Hub>,
    stream: TcpStream,
    token: u64,
    role: Role,
    mut outgoing: mpsc::Receiver<Wire>,
    replies: mpsc::WeakSender<Wire>,
    cancel: &mut watch::Receiver<bool>,
) -> Option<String> {
    if !is_current(hub, token, cancel) {
        return None;
    }
    let observer = cancel.clone();
    let (mut reader, mut writer) = stream.into_split();
    let (reject_sender, reject_receiver) = oneshot::channel();
    let mut reject_sender = Some(reject_sender);
    let mut reader_done = false;
    let mut reading = pin!(read_messages(
        &mut reader,
        hub,
        token,
        role,
        replies,
        &observer,
    ));
    let mut writing = pin!(write_messages(&mut writer, &mut outgoing, reject_receiver));
    let mut stopping = pin!(cancelled(cancel));

    poll_fn(|cx| {
        if stopping.as_mut().poll(cx).is_ready() || !is_current(hub, token, &observer) {
            return Poll::Ready(None);
        }
        if !reader_done {
            match reading.as_mut().poll(cx) {
                Poll::Ready(ReadExit::Stopped) => return Poll::Ready(None),
                Poll::Ready(ReadExit::Lost(message)) => return Poll::Ready(Some(message)),
                Poll::Ready(ReadExit::Reject(message)) => {
                    reader_done = true;
                    if let Some(sender) = reject_sender.take() {
                        let _ = sender.send(message);
                    }
                }
                Poll::Pending => {}
            }
        }
        // 协议拒绝也交给唯一写任务，避免在已有半帧中间插入错误帧。
        writing.as_mut().poll(cx).map(Some)
    })
    .await
}

async fn read_messages<R: AsyncRead + Unpin>(
    reader: &mut R,
    hub: &Arc<Hub>,
    token: u64,
    role: Role,
    replies: mpsc::WeakSender<Wire>,
    cancel: &watch::Receiver<bool>,
) -> ReadExit {
    let mut received_snapshot = false;
    loop {
        let wire = match read_frame(reader, MAX_FRAME_BYTES).await {
            Ok(wire) => wire,
            Err(message) => return ReadExit::Lost(message),
        };
        if !is_current(hub, token, cancel) {
            return ReadExit::Stopped;
        }
        match wire {
            Wire::Hello { .. } => {
                return ReadExit::Reject("握手完成后不能重复发送 hello".to_owned());
            }
            Wire::Ping => {
                let Some(sender) = replies.upgrade() else {
                    return ReadExit::Lost("发送队列已关闭".to_owned());
                };
                if let Err(error) = sender.try_send(Wire::Pong) {
                    return ReadExit::Lost(queue_error(error));
                }
            }
            Wire::Pong => {}
            Wire::Error { message } => {
                // 未取得初始快照时的错误代表入房被拒绝；动作拒绝只提示。
                if matches!(role, Role::Guest) && !received_snapshot {
                    return ReadExit::Lost(message);
                }
                hub.network_notice(token, message);
            }
            Wire::State { snapshot } => match role {
                Role::Host { .. } => {
                    return ReadExit::Reject("客机不能发送房间快照".to_owned());
                }
                Role::Guest => {
                    if let Err(message) = hub.receive_snapshot(token, *snapshot) {
                        return ReadExit::Reject(message);
                    }
                    received_snapshot = true;
                }
            },
            Wire::Action { request } => match role {
                Role::Host { peer_id } => hub.receive_peer(token, peer_id, request),
                Role::Guest => {
                    return ReadExit::Reject("房主不能向客机发送玩家操作".to_owned());
                }
            },
        }
    }
}

async fn write_messages<W: AsyncWrite + Unpin>(
    writer: &mut W,
    outgoing: &mut mpsc::Receiver<Wire>,
    mut rejection: oneshot::Receiver<String>,
) -> String {
    let mut heartbeat = time::interval_at(
        Instant::now() + HEARTBEAT_INTERVAL,
        HEARTBEAT_INTERVAL,
    );
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        let event = {
            let mut tick = pin!(heartbeat.tick());
            poll_fn(|cx| {
                match Pin::new(&mut rejection).poll(cx) {
                    Poll::Ready(Ok(message)) => return Poll::Ready(WriteEvent::Reject(message)),
                    Poll::Ready(Err(_)) => return Poll::Ready(WriteEvent::Closed),
                    Poll::Pending => {}
                }
                if outgoing.is_closed() {
                    return Poll::Ready(WriteEvent::Closed);
                }
                if outgoing.capacity() == 0 {
                    return Poll::Ready(WriteEvent::Full);
                }
                if tick.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(WriteEvent::Frame(Wire::Ping));
                }
                match outgoing.poll_recv(cx) {
                    Poll::Ready(Some(wire)) => Poll::Ready(WriteEvent::Frame(wire)),
                    Poll::Ready(None) => Poll::Ready(WriteEvent::Closed),
                    Poll::Pending => Poll::Pending,
                }
            })
            .await
        };
        match event {
            WriteEvent::Closed => return "发送队列已关闭".to_owned(),
            WriteEvent::Full => return "发送队列已满，连接已关闭".to_owned(),
            WriteEvent::Frame(wire) => {
                if let Err(message) = write_frame(writer, &wire, MAX_FRAME_BYTES).await {
                    return message;
                }
            }
            WriteEvent::Reject(message) => {
                let wire = Wire::Error {
                    message: message.clone(),
                };
                return match write_frame(writer, &wire, MAX_FRAME_BYTES).await {
                    Ok(()) => message,
                    Err(error) => format!("{message}；{error}"),
                };
            }
        }
    }
}

fn queue_error(error: mpsc::error::TrySendError<Wire>) -> String {
    match error {
        mpsc::error::TrySendError::Full(_) => "发送队列已满，连接已关闭".to_owned(),
        mpsc::error::TrySendError::Closed(_) => "发送队列已关闭".to_owned(),
    }
}

async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R, limit: usize) -> Result<Wire, String> {
    // 一次 future 完整持有帧头与帧体读取，心跳不会取消并重启半包读取。
    time::timeout(FRAME_TIMEOUT, async {
        let mut header = [0_u8; 4];
        reader
            .read_exact(&mut header)
            .await
            .map_err(|error| format!("读取消息长度失败：{error}"))?;
        let length = u32::from_be_bytes(header) as usize;
        if length == 0 || length > limit || length > MAX_FRAME_BYTES {
            return Err("消息长度无效或超过上限".to_owned());
        }
        let mut payload = vec![0_u8; length];
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|error| format!("读取消息内容失败：{error}"))?;
        serde_json::from_slice(&payload).map_err(|error| format!("消息格式无效：{error}"))
    })
    .await
    .map_err(|_| "接收消息超时，连接已断开".to_owned())?
}

async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    wire: &Wire,
    limit: usize,
) -> Result<(), String> {
    let payload = serde_json::to_vec(wire).map_err(|error| format!("序列化消息失败：{error}"))?;
    if payload.is_empty() || payload.len() > limit || payload.len() > MAX_FRAME_BYTES {
        return Err("发送消息超过长度上限".to_owned());
    }
    let header = (payload.len() as u32).to_be_bytes();
    time::timeout(FRAME_TIMEOUT, async {
        writer
            .write_all(&header)
            .await
            .map_err(|error| format!("发送消息长度失败：{error}"))?;
        writer
            .write_all(&payload)
            .await
            .map_err(|error| format!("发送消息内容失败：{error}"))?;
        Ok(())
    })
    .await
    .map_err(|_| "发送消息超时，连接已断开".to_owned())?
}

fn is_current(hub: &Hub, token: u64, cancel: &watch::Receiver<bool>) -> bool {
    !*cancel.borrow() && cancel.has_changed().is_ok() && hub.network_is_current(token)
}

async fn cancelled(cancel: &mut watch::Receiver<bool>) {
    loop {
        if *cancel.borrow() {
            return;
        }
        if cancel.changed().await.is_err() {
            return;
        }
    }
}

async fn until_cancelled<F: Future>(
    cancel: &mut watch::Receiver<bool>,
    future: F,
) -> Option<F::Output> {
    let mut stopping = pin!(cancelled(cancel));
    let mut operation = pin!(future);
    poll_fn(|cx| {
        if stopping.as_mut().poll(cx).is_ready() {
            Poll::Ready(None)
        } else {
            operation.as_mut().poll(cx).map(Some)
        }
    })
    .await
}
