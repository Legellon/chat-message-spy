mod handlers {
    use super::*;

    pub(super) async fn handle_get(
        _req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        let mut res = Response::builder().header(CONTENT_TYPE, "text/html");

        let body;

        match tokio::fs::read("src/index.html").await {
            Ok(c) => {
                res = res.status(StatusCode::OK);
                body = Full::new(c.into());
            }
            Err(e) => {
                res = res.status(StatusCode::INTERNAL_SERVER_ERROR);
                body = Full::new(Bytes::from(format!("internal server error: {}", e)));
            }
        };

        Ok(res.body(body.into()).unwrap())
    }

    pub(super) async fn handle_post(
        req: Request<Incoming>,
        tx: ShutdownSender<String>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
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

    pub(super) async fn handle_not_found(
        _req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, Infallible> {
        let res = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from("404: not found")))
            .unwrap();
        Ok(res)
    }
}

use self::handlers::*;
use crate::match_pattern::{MatchMode, MatchPattern};
use crate::network::{ExpectResult, ServeExpectHandler, ShutdownSender};

use crate::protocol::{ActionRes, TwitchAction};
use crate::{AppEvent, TwitchEvent};
use futures::{future::BoxFuture, SinkExt, StreamExt};
use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    header::CONTENT_TYPE,
    server::conn::http1,
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const TWITCH_USER_ACCESS_TOKEN: &str = "Twitch-User-Access-Token";
//TODO: We can store this token in public, but good to move to .env or some kind of config for better configuration
const TWITCH_CLIENT_ID: &str = "85ningw35fofi86ue5bbahw22xsazw";
const TWITCH_CHAT_URI: &str = "ws://irc-ws.chat.twitch.tv:80";
const TWITCH_AUTH_ENDPOINT: &str = "https://id.twitch.tv/oauth2/authorize";
const TWITCH_VALIDATE_ENDPOINT: &str = "https://id.twitch.tv/oauth2/validate";
const ANONYMOUS_LOGIN: &str = "justinfan1337";

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
        stream: TcpStream,
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

pub struct TwitchConnectionCmd {
    pub action: TwitchAction,
    pub responder: tokio::sync::oneshot::Sender<ActionRes>,
}

pub enum TwitchConnectionEvent {
    Message(String),
}

pub fn spawn_twitch_irc(
    emitter: tokio::sync::mpsc::Sender<AppEvent>,
    mut channels: Option<&[&str]>,
) -> mpsc::UnboundedSender<TwitchConnectionCmd> {
    let (cmd_sender, cmd_receiver) = mpsc::unbounded_channel();

    let channels = channels
        .take()
        .unwrap_or_default()
        .into_iter()
        .map(|&s| s.to_owned())
        .collect();

    tokio::spawn(async_connect_twitch_irc(
        "",
        ANONYMOUS_LOGIN,
        channels,
        emitter,
        cmd_receiver,
    ));

    cmd_sender
}

//TODO: Add more verbose error handling
async fn async_connect_twitch_irc(
    token: &str,
    login: &str,
    channels: Vec<String>,
    emitter: tokio::sync::mpsc::Sender<AppEvent>,
    input_rx: mpsc::UnboundedReceiver<TwitchConnectionCmd>,
) -> Result<(), ()> {
    let (stream, _) = connect_async(TWITCH_CHAT_URI).await.unwrap();
    let (mut write, mut read) = stream.split();

    write
        .send(Message::Text(format!("PASS oauth:{}", token)))
        .await
        .unwrap();
    write
        .send(Message::Text(format!("NICK {}", login)))
        .await
        .unwrap();

    match read.next().await.unwrap() {
        Ok(Message::Text(s)) => {
            //TODO: Handle failed auth with twitch chat server
        }
        Err(e) => panic!(
            "ERROR: failed to send message to twitch while authentication: {}",
            e
        ),
        any => panic!("ERROR: unknown exception on twitch chat auth: {:?}", any),
    }

    for channel in channels {
        write
            .send(Message::Text(format!("JOIN #{}", channel)))
            .await
            .unwrap();
    }

    while let Some(Ok(m)) = read.next().await {
        match m {
            Message::Text(s) => {
                if !s.starts_with(':') {
                    let s_parts: Vec<_> = s.split(' ').collect();
                    match &s_parts[..] {
                        ["PING", uri, ..] => write
                            .send(Message::Text(format!("PONG {}", uri)))
                            .await
                            .unwrap(),
                        sl => {
                            unreachable!("ERROR: unknown twitch chat command: {:?}", sl)
                        }
                    }
                    continue;
                }
                let _ = emitter
                    .send(AppEvent::Twitch(TwitchEvent::Message(s)))
                    .await;
            }
            m => unreachable!(
                "ERROR: unknown message type from {}: {}",
                TWITCH_CHAT_URI, m
            ),
        }
    }

    Ok(())
}
