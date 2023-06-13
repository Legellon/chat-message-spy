mod twitch_auth_handlers {
    use crate::network::ShutdownSender;
    use crate::twitch::TWITCH_USER_ACCESS_TOKEN;
    use http_body_util::Full;
    use hyper::body::{Bytes, Incoming};
    use hyper::header::CONTENT_TYPE;
    use hyper::{Request, Response, StatusCode};
    use std::convert::Infallible;

    type HandlerReq = Request<Incoming>;
    type HandlerRes = Result<Response<Full<Bytes>>, Infallible>;

    pub(super) async fn handle_get(_: HandlerReq) -> HandlerRes {
        let mut res = Response::builder().header(CONTENT_TYPE, "text/html");

        let body;

        match tokio::fs::read("src/index.html").await {
            Ok(c) => {
                res = res.status(StatusCode::OK);
                body = Full::new(c.into());
            }
            Err(e) => {
                res = res.status(StatusCode::INTERNAL_SERVER_ERROR);
                body = Full::new(Bytes::from(format!("500: internal server error: {}", e)));
            }
        };

        Ok(res.body(body).unwrap())
    }

    pub(super) async fn handle_post(req: HandlerReq, tx: ShutdownSender<String>) -> HandlerRes {
        let mut res = Response::builder();

        if let Some(token) = req.headers().get(TWITCH_USER_ACCESS_TOKEN) {
            res = res.status(StatusCode::OK).header("Connection", "close");
            let _ = tx
                .lock()
                .unwrap()
                .take()
                .unwrap()
                .send(String::from(token.to_str().unwrap()));
        } else {
            res = res.status(StatusCode::BAD_REQUEST);
        }

        Ok(res.body(Full::new(Bytes::new())).unwrap())
    }

    pub(super) async fn handle_not_found(_: HandlerReq) -> HandlerRes {
        let res = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from("404: not found")))
            .unwrap();
        Ok(res)
    }
}

use self::twitch_auth_handlers::*;
use crate::network::{ExpectResult, ServeExpectHandler, ShutdownSender};
use crate::protocol::{ActionRes, PartAction, TwitchAction};
use crate::{AppEvent, AppEventEmitter, TwitchEvent};
use fnv::{FnvHashMap, FnvHashSet};
use futures::stream::{SplitSink, SplitStream};
use futures::{future::BoxFuture, SinkExt, StreamExt};
use hyper::{server::conn::http1, service::service_fn, Method};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

const TWITCH_USER_ACCESS_TOKEN: &str = "Twitch-User-Access-Token";
const TWITCH_CLIENT_ID: &str = "85ningw35fofi86ue5bbahw22xsazw";
const TWITCH_CHAT_URI: &str = "ws://irc-ws.chat.twitch.tv:80";
const TWITCH_AUTH_ENDPOINT: &str = "https://id.twitch.tv/oauth2/authorize";
const TWITCH_VALIDATE_ENDPOINT: &str = "https://id.twitch.tv/oauth2/validate";
const ANONYMOUS_LOGIN: &str = "justinfan1337";

type WriteHalf = SplitSink<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, Message>;
type ReadHalf = SplitStream<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>;
pub type TwitchCmdReceiver = mpsc::Receiver<TwitchCmd>;
pub type TwitchCmdSender = mpsc::Sender<TwitchCmd>;
pub type ActionResponder = tokio::sync::oneshot::Sender<ActionRes>;

#[derive(Deserialize, Serialize)]
struct ValidatedTokenResponse {
    client_id: String,
    login: String,
    scopes: Vec<String>,
    user_id: String,
    expires_in: u32,
}

struct TwitchAuthHandler;

impl ServeExpectHandler for TwitchAuthHandler {
    type Output = String;

    fn expect_handler(
        stream: tokio::net::TcpStream,
        tx: ShutdownSender<Self::Output>,
    ) -> BoxFuture<'static, ExpectResult<()>> {
        Box::pin(async move {
            let connection = http1::Builder::new().serve_connection(
                stream,
                service_fn(|req| async {
                    match (req.method(), req.uri().path()) {
                        (&Method::GET, "/") => handle_get(req).await,
                        (&Method::POST, "/") => handle_post(req, tx.clone()).await,
                        _ => handle_not_found(req).await,
                    }
                }),
            );

            if let Err(e) = connection.await {
                panic!("{}", e);
            };

            Ok(())
        })
    }
}

pub enum TwitchInfoCmd {
    Channels,
}

pub enum TwitchCmdType {
    Connection(TwitchAction),
    Info(TwitchInfoCmd),
}

pub struct TwitchCmd {
    pub action: TwitchCmdType,
    pub responder: ActionResponder,
}

pub enum TwitchConnectionEvent {
    Message(String),
}

pub static CHANNELS: LazyLock<RwLock<FnvHashMap<&'static str, Vec<String>>>> =
    LazyLock::new(|| RwLock::new(FnvHashMap::default()));

pub fn spawn_twitch_irc(
    emitter: AppEventEmitter,
    channels: Option<Vec<String>>,
) -> TwitchCmdSender {
    let (cmd_sender, cmd_receiver) = mpsc::channel(16);

    let channels = channels.unwrap_or_default();

    let _ = tokio::spawn(async_connect_twitch_irc(
        "",
        ANONYMOUS_LOGIN,
        channels,
        emitter,
        cmd_receiver,
    ));

    cmd_sender
}

async fn join(w: &mut WriteHalf, channel: String) {
    let mut lock = CHANNELS.write().await;
    w.send(Message::Text(format!("JOIN #{}", channel)))
        .await
        .unwrap();
    lock.entry("twitch")
        .and_modify(|v| v.push(channel.clone()))
        .or_insert(vec![channel]);
}

async fn join_many(w: &mut WriteHalf, channels: Vec<String>) {
    let mut lock = CHANNELS.write().await;
    for channel in channels {
        w.send(Message::Text(format!("JOIN #{}", channel)))
            .await
            .unwrap();
        lock.entry("twitch")
            .and_modify(|v| v.push(channel.clone()))
            .or_insert(vec![channel]);
    }
}

async fn auth(w: &mut WriteHalf, _: &mut ReadHalf, token: &str, login: &str) {
    w.send(Message::Text(format!("PASS oauth:{}", token)))
        .await
        .unwrap();
    w.send(Message::Text(format!("NICK {}", login)))
        .await
        .unwrap();
}

async fn part<'a>(w: &mut WriteHalf, channel: &'a str) -> Option<&'a str> {
    let mut lock = CHANNELS.write().await;
    let mut res = None;

    let opt = lock.get_mut("twitch").and_then(|v| {
        v.iter()
            .position(|x| x == channel)
            .and_then(|p| Some(v.swap_remove(p)))
    });

    if let Some(ch) = opt {
        w.send(Message::Text(format!("PART #{}", ch)))
            .await
            .unwrap();
        res = Some(channel);
    }

    res
}

async fn part_many(w: &mut WriteHalf, channels: &[String]) -> Vec<String> {
    let mut lock = CHANNELS.write().await;
    let opt = lock.get_mut("twitch");
    let mut res = vec![];

    if let Some(v) = opt {
        for channel in channels {
            w.send(Message::Text(format!("PART #{}", channel)))
                .await
                .unwrap();

            v.iter()
                .position(|x| x == channel)
                .and_then(|p| Some(v.swap_remove(p)))
                .and_then(|s| {
                    res.push(s);
                    Some(())
                });
        }
    }

    res
}

enum IrcMessage {
    Ping(String),
    Privmsg(String),
}

fn split_msg(m: Message) -> Vec<IrcMessage> {
    match m {
        Message::Text(s) => s
            .split("\r\n")
            .filter(|s| !s.is_empty())
            .map(|s| {
                if !s.starts_with(':') {
                    let s_parts: Vec<_> = s.split(' ').collect();
                    match &s_parts[..] {
                        ["PING", uri, ..] => IrcMessage::Ping(format!("PONG {}", uri)),
                        any => unreachable!("ERROR: unknown twitch chat command: {:?}", any),
                    }
                } else {
                    IrcMessage::Privmsg(s.to_owned())
                }
            })
            .collect(),
        m => unreachable!(
            "ERROR: unknown message type from {}: {}",
            TWITCH_CHAT_URI, m
        ),
    }
}

async fn handle_irc(w: &mut WriteHalf, e: &AppEventEmitter, m: IrcMessage) {
    match m {
        IrcMessage::Ping(pong) => {
            let _ = w.send(Message::Text(pong)).await;
        }
        IrcMessage::Privmsg(m) => {
            let _ = e.send(AppEvent::Twitch(TwitchEvent::Message(m)));
        }
    }
}

async fn handle_cmd(w: &mut WriteHalf, action: TwitchCmdType) -> ActionRes {
    match action {
        TwitchCmdType::Connection(action) => match action {
            TwitchAction::Join(channels) => {
                join_many(w, channels).await;
                ActionRes::Success
            }
            TwitchAction::Part(a) => match a {
                PartAction::Some(channels) => {
                    part_many(w, &channels).await;
                    ActionRes::Success
                }
                PartAction::All => unreachable!(),
            },
            TwitchAction::Start(_) => unreachable!(),
        },
        TwitchCmdType::Info(action) => match action {
            TwitchInfoCmd::Channels => {
                let lock = CHANNELS.read().await;
                ActionRes::Data(serde_json::to_string_pretty(&lock.to_owned()).unwrap())
            }
        },
    }
}

async fn async_connect_twitch_irc(
    token: &str,
    login: &str,
    channels: Vec<String>,
    emitter: AppEventEmitter,
    mut cmd_receiver: TwitchCmdReceiver,
) -> Result<(), ()> {
    let (stream, _) = connect_async(TWITCH_CHAT_URI).await.unwrap();
    let (mut write, mut read) = stream.split();

    auth(&mut write, &mut read, token, login).await;
    join_many(&mut write, channels).await;

    loop {
        tokio::select! {
            irc_msg = read.next() => {
                if let Some(Ok(msg)) = irc_msg {
                    let msgs = split_msg(msg);
                    for m in msgs {
                        handle_irc(&mut write, &emitter, m).await;
                    }
                } else {
                    break;
                }
            },
            cmd = cmd_receiver.recv() => {
                let cmd = cmd.unwrap();
                let TwitchCmd { action, responder } = cmd;

                if let TwitchCmdType::Connection(TwitchAction::Part(PartAction::All)) = action {
                    let _ = responder.send(ActionRes::Success);
                    break;
                }

                let res = handle_cmd(&mut write, action).await;
                let _ = responder.send(res);
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
pub struct UserMessage {
    pub channel: String,
    pub author: String,
    pub message: String,
}

pub fn parse_privmsg(s: &str) -> Option<UserMessage> {
    let mut splitn = s.splitn(3, ':');
    let _ = splitn.next()?;

    let mut prefix = splitn.next()?.split_terminator(' ');

    let author = prefix.next()?.split('!').next()?.to_owned();
    let _ = prefix.next()?;
    let channel = prefix.next()?[1..].to_owned();

    let message = splitn.next()?.to_owned();

    Some(UserMessage {
        channel,
        author,
        message,
    })
}
