use chatspy::match_pattern::{MatchMode, MatchPattern};
use chatspy::protocol::*;
use chatspy::storage::run_init_migration;
use chatspy::twitch::{spawn_twitch_irc, TwitchConnectionCmd};
use chatspy::{AppEvent, LockedDefaultPattern, PatternStorage, SOCKET_PATH, TwitchEvent};
use std::io::ErrorKind::NotFound;
use std::path::Path;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
// fn twitch_auth_uri(port: u16) -> String {
//     format!(
//         "{}?response_type=token&client_id={}&redirect_uri={}://localhost:{}&scope=chat%3Aread",
//         TWITCH_AUTH_ENDPOINT, TWITCH_CLIENT_ID, PROTOCOL, port
//     )
// }

// async fn validate_twitch_token(token: &str) -> Option<String> {
//     let client = reqwest::Client::new();
//     let res = client
//         .get(TWITCH_VALIDATE_ENDPOINT)
//         .header(reqwest::header::AUTHORIZATION, format!("OAuth {}", token))
//         .send()
//         .await;
//
//     if let Ok(r) = res.unwrap().json::<ValidatedTokenResponse>().await {
//         Some(r.login)
//     } else {
//         None
//     }
// }

// async fn get_twitch_token(port: u16) -> String {
//     let addr = SocketAddr::from(([127, 0, 0, 1], port));
//     let tcp_listener = TcpListener::bind(addr).await.unwrap();
//
//     let (tx, mut rx) = mpsc::channel::<String>(1);
//
//     loop {
//         let (stream, _) = tcp_listener.accept().await.unwrap();
//
//         let tx = tx.clone();
//         tokio::spawn(async move {
//             handle_connection(stream, &tx).await;
//         });
//
//         if let Some(token) = rx.recv().await {
//             break token;
//         }
//     }
// }

const TEST_CHANNELS: [&str; 33] = [
    "nix",
    "just_ns",
    "praden",
    "admiralbulldog",
    "woowakgood",
    "kato_junichi0817",
    "esl_dota2",
    "riotgames",
    "honeymad",
    "jinnytty",
    "jingburger",
    "paragon_dota",
    "jasper7se",
    "fps_shaka",
    "krabick",
    "xqc",
    "forsen",
    "segall",
    "uselessmouth",
    "jesusavgn",
    "mizkif",
    "cellbit",
    "lirik",
    "drututt",
    "roadhouse",
    "shroud",
    "forever",
    "philza",
    "valorant_jpn",
    "paka9999",
    "broxah",
    "loltyler1",
    "tarik",
];

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let sqlite_conn = rusqlite::Connection::open_in_memory().unwrap();
    run_init_migration(&sqlite_conn);

    let pattern_storage = PatternStorage::new();

    let (event_emitter, mut event_receiver) = tokio::sync::mpsc::channel(256);

    spawn_socket(event_emitter.clone())?;
    let twitch_cmd_sender = spawn_twitch_irc(event_emitter.clone(), Some(&TEST_CHANNELS));

    let processor_sender = spawn_processor(pattern_storage.default_pattern());

    while let Some(e) = event_receiver.recv().await {
        match e {
            AppEvent::Twitch(e) => match e {
                TwitchEvent::Message(m) => {
                    let _ = processor_sender.send(m);
                }
            },
            AppEvent::ExternalAction { action, responder } => match action {
                Action::Twitch(a) => {
                    let _ = twitch_cmd_sender.send(TwitchConnectionCmd {
                        action: a,
                        responder,
                    });
                }
                Action::Add(a) => match a {
                    AddAction::Pattern {
                        name,
                        raw_pattern: rp,
                        default,
                    } => {
                        let p = MatchPattern::builder().words(rp.0).mode(rp.1).build();
                        pattern_storage.add(name, p, default).unwrap();
                        let _ = responder.send(ActionRes::Success);
                    }
                },
                Action::Kill => event_receiver.close(),
            },
            AppEvent::Error => { panic!("something went wrong") }
        };
    }

    close_socket()?;

    Ok(())
}

fn spawn_processor(active_pattern: LockedDefaultPattern) -> crossbeam::channel::Sender<String> {
    let (msg_sender, msg_receiver) = crossbeam::channel::unbounded::<String>();
    let _ = std::thread::spawn(move || {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();

        for msg in msg_receiver {
            let lock = active_pattern.read().unwrap();
            if let Some(p) = lock.clone() {
                pool.spawn(move || {
                    if p.read().unwrap().match_str(&msg) {
                        print!("{}", msg)
                    }
                })
            }
        }
    });
    msg_sender
}

fn spawn_socket(emitter: tokio::sync::mpsc::Sender<AppEvent>) -> std::io::Result<()> {
    close_socket()?;
    let _ = tokio::spawn(async move {
        let ul = UnixListener::bind(SOCKET_PATH)?;
        while let Ok((mut stream, _)) = ul.accept().await {
            let mut buf = vec![];

            let _ = stream.read_to_end(&mut buf).await?;
            let action = serde_json::from_slice::<Action>(&buf)?;

            let (responder, res_receiver) = tokio::sync::oneshot::channel();
            let _ = emitter.send(AppEvent::ExternalAction { action, responder }).await;
            let res = res_receiver.await.unwrap();

            stream.write_all(&serde_json::to_vec(&res)?).await?;
            stream.shutdown().await?;
        }
        Ok::<(), std::io::Error>(())
    });
    Ok(())
}

fn close_socket() -> std::io::Result<()> {
    let socket = Path::new(SOCKET_PATH);
    if socket.exists() {
        std::fs::remove_file(socket)?;
    }
    Ok(())
}