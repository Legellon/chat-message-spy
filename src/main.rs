use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{connect, Message};
use url::Url;

const TWITCH_USER_ACCESS_TOKEN: &str = "Twitch-User-Access-Token";

const TWITCH_CLIENT_ID: &str = "85ningw35fofi86ue5bbahw22xsazw";

const TWITCH_CHAT_URI: &str = "ws://irc-ws.chat.twitch.tv:80";
const TWITCH_AUTH_ENDPOINT: &str = "https://id.twitch.tv/oauth2/authorize";
const TWITCH_VALIDATE_ENDPOINT: &str = "https://id.twitch.tv/oauth2/validate";

const PROTOCOL: &str = "http";
const RESERVED_PORTS: [u16; 3] = [16728, 39561, 24329];

#[derive(Deserialize, Serialize)]
struct TwitchSuccessfulValidateRes {
    client_id: String,
    login: String,
    scopes: Vec<String>,
    user_id: String,
    expires_in: u32,
}

fn twitch_auth_uri(port: u16) -> String {
    format!(
        "{}?response_type=token&client_id={}&redirect_uri={}://localhost:{}&scope=chat%3Aread",
        TWITCH_AUTH_ENDPOINT, TWITCH_CLIENT_ID, PROTOCOL, port
    )
}

async fn validate_twitch_token(token: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let res = client
        .get(TWITCH_VALIDATE_ENDPOINT)
        .header(reqwest::header::AUTHORIZATION, format!("OAuth {}", token))
        .send()
        .await;

    if let Ok(r) = res.unwrap().json::<TwitchSuccessfulValidateRes>().await {
        Some(r.login)
    } else {
        None
    }
}

async fn process_req(
    req: Request<hyper::body::Incoming>,
    tx: &mpsc::Sender<String>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            let mut res = Response::builder().header("Content-Type", "text/html");

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
        (&Method::POST, "/") => {
            let mut res = Response::builder();

            if let Some(token) = req.headers().get(TWITCH_USER_ACCESS_TOKEN) {
                res = res.status(StatusCode::OK).header("Connection", "close");
                let _ = tx.send(String::from(token.to_str().unwrap())).await;
            } else {
                res = res.status(StatusCode::BAD_REQUEST);
            }

            Ok(res.body(Full::new(Bytes::new())).unwrap())
        }
        _ => {
            let res = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Full::new(Bytes::from("404: not found")))
                .unwrap();

            Ok(res)
        }
    }
}

async fn handle_tcp_conn(stream: TcpStream, tx: &mpsc::Sender<String>) {
    let service = service_fn(|req| async move { process_req(req, tx).await });
    let conn = http1::Builder::new().serve_connection(stream, service);
    if let Err(e) = conn.await {
        panic!("connection error: {}", e);
    }
}

fn insert_twitch_message(conn: &rusqlite::Connection, message: TwitchMessage) {}

#[derive(Debug, Clone)]
struct TwitchToken {
    id: u64,
    token: String,
    login: String,
}

fn create_token_table(conn: &rusqlite::Connection) {
    if let Err(e) = conn.execute(
        "CREATE TABLE token (id INTEGER PRIMARY KEY, token TEXT NOT NULL, login TEXT NOT NULL)",
        (),
    ) {
        eprintln!("failed to create table 'token': {}", e);
    }
}

fn insert_twitch_token(conn: &rusqlite::Connection, token: &str, login: &str) {
    conn.execute(
        "INSERT INTO token (token, login) VALUES (?1, ?2)",
        (token, login),
    )
        .unwrap();
}

fn get_stored_users(db_conn: &rusqlite::Connection) -> Vec<TwitchToken> {
    match db_conn.prepare("SELECT * FROM token") {
        Ok(mut s) => s
            .query_map([], |row| {
                Ok(TwitchToken {
                    id: row.get(0).unwrap(),
                    token: row.get(1).unwrap(),
                    login: row.get(2).unwrap(),
                })
            })
            .unwrap()
            .map(|u| u.unwrap())
            .collect(),
        Err(e) => panic!("failed to select from 'token': {}", e),
    }
}

#[derive(Debug)]
struct TwitchMessage {
    id: u64,
    author: String,
    content: String,
    chat: String,
    time: String,
}

fn create_message_table(conn: &rusqlite::Connection) {
    if let Err(e) = conn.execute(
        "CREATE TABLE message (\
        id      INTEGER PRIMARY KEY,\
        author  TEXT NOT NULL,\
        content TEXT NOT NULL,\
        chat    TEXT NOT NULL,\
        time    TIMESTAMP\
        )",
        (),
    ) {
        eprintln!("failed to create table 'message': {}", e);
    }
}

async fn get_twitch_token(port: u16) -> String {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let tcp_listener = TcpListener::bind(addr).await.unwrap();

    let (tx, mut rx) = mpsc::channel::<String>(1);

    loop {
        let (stream, _) = tcp_listener.accept().await.unwrap();

        let tx = tx.clone();
        tokio::spawn(async move {
            handle_tcp_conn(stream, &tx).await;
        });

        if let Some(token) = rx.recv().await {
            break token;
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let sqlite_conn = rusqlite::Connection::open("storage.sqlite").unwrap();
    // let sqlite_conn = Connection::open_in_memory().unwrap();

    create_token_table(&sqlite_conn);
    create_message_table(&sqlite_conn);

    let mut tokens = get_stored_users(&sqlite_conn);

    if tokens.len() == 0 {
        let task = tokio::spawn(async move {
            println!("{}", twitch_auth_uri(RESERVED_PORTS[0]));
            let token = get_twitch_token(RESERVED_PORTS[0]).await;
            let login = validate_twitch_token(token.as_str()).await;
            (token, login)
        });

        match task.await {
            Ok((token, Some(login))) => insert_twitch_token(&sqlite_conn, &token, &login),
            Ok((_, None)) => panic!("twitch token isn't valid"),
            Err(e) => panic!("{}", e),
        }

        tokens = get_stored_users(&sqlite_conn);
    }

    let target_token = if tokens.len() == 1 {
        &tokens[0]
    } else {
        unimplemented!("can't choose from many tokens yet");
    };

    let ws_uri = Url::parse(TWITCH_CHAT_URI).unwrap();
    let (mut socket, _) = connect(ws_uri).unwrap();

    println!("used token: {}", target_token.token);

    socket
        .write_message(Message::Text(format!("PASS oauth:{}", target_token.token)).into())
        .unwrap();

    socket
        .write_message(Message::Text(format!("NICK {}", target_token.login)).into())
        .unwrap();

    let target_channel = "xqc";

    socket
        .write_message(Message::Text(format!("JOIN #{}", target_channel)).into())
        .unwrap();

    loop {
        let message = socket.read_message().unwrap();
        match &message {
            Message::Text(s) => {
                if s.chars().nth(0) != Some(':') {
                    let s_parts: Vec<_> = s.split(' ').collect();
                    match s_parts[0] {
                        "PING" => socket
                            .write_message(Message::Text(format!("PONG {}", s_parts[1]).into()))
                            .unwrap(),
                        c => unreachable!("unknown twitch chat command: {}", c),
                    }
                    continue;
                }
                print!("{}", message);
            }
            m => unreachable!("unknown message type from {}: {}", TWITCH_CHAT_URI, m),
        };
    }
}