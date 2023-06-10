use chatspy::match_pattern::MatchPattern;
use chatspy::protocol::*;
use chatspy::storage::{get_messages, insert_message, run_init_migration};
use chatspy::twitch::{AppEventEmitter, parse_privmsg, spawn_twitch_irc, TwitchConnectionCmd};
use chatspy::{AppEvent, PatternStorage, TwitchEvent, SOCKET_PATH, TWITCH_DB_PATH};
use clap::Parser;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

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

#[derive(Parser)]
struct Args {
    #[arg(short, long, value_parser, num_args=1.., value_delimiter = ',')]
    channels: Option<Vec<String>>,
    #[arg(short, long)]
    test: Option<bool>,
}

fn open_sqlite() {
    let sqlt = rusqlite::Connection::open(TWITCH_DB_PATH).unwrap();
    run_init_migration(&sqlt);
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let prejoin = if args.test.unwrap_or_default() {
        Some(TEST_CHANNELS.into_iter().map(|s| s.to_owned()).collect())
    } else {
        args.channels
    };

    open_sqlite();
    let pattern_storage = Arc::new(PatternStorage::new());
    let (event_emitter, event_receiver) = crossbeam::channel::bounded(128);

    spawn_socket(event_emitter.clone())?;
    let twitch_cmd_sender = spawn_twitch_irc(event_emitter.clone(), prejoin);
    let processor_sender = spawn_processor(pattern_storage.clone());

    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel();
    let kill_tx = Arc::new(Mutex::new(Some(kill_tx)));

    let _ = std::thread::spawn(move || {
        while let Ok(e) = event_receiver.recv() {
            match e {
                AppEvent::Twitch(e) => match e {
                    TwitchEvent::Message(m) => {
                        let _ = processor_sender.send(m);
                    }
                },
                AppEvent::ExternalAction { action, responder } => match action {
                    Action::Twitch(a) => {
                        let _ = twitch_cmd_sender.blocking_send(TwitchConnectionCmd {
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
                            let pattern_storage = pattern_storage.clone();
                            tokio::task::block_in_place(move || {
                                let p = MatchPattern::builder().words(rp.0).mode(rp.1).build();
                                let _ = pattern_storage.add(name, p, default);
                                let _ = responder.send(ActionRes::Success);
                            });
                        }
                    },
                    Action::Get(a) => match a {
                        GetAction::Messages { channel, author } => {
                            tokio::task::block_in_place(move || {
                                let sqlt = rusqlite::Connection::open(TWITCH_DB_PATH).unwrap();
                                let vm = get_messages(&sqlt, author, channel);
                                let res = serde_json::to_string_pretty(&vm).unwrap();
                                let _ = responder.send(ActionRes::Data(res));
                            });
                        }
                    },
                    Action::Kill => {
                        let _ = kill_tx.clone().lock().unwrap().take().unwrap().send(());
                    }
                },
                AppEvent::Error => {
                    panic!("something went wrong");
                }
            };
        }
    });

    let _ = kill_rx.await;
    close_socket()?;

    Ok(())
}

fn spawn_processor(pattern_storage: Arc<PatternStorage>) -> crossbeam::channel::Sender<String> {
    let (msg_sender, msg_receiver) = crossbeam::channel::bounded::<String>(64);
    let _ = std::thread::spawn(move || {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();

        for msg in msg_receiver {
            let lock = pattern_storage.default_pattern().read().unwrap();

            if let Some(p) = lock.clone() {
                pool.spawn(move || {
                    if let Some(privmsg) = parse_privmsg(&msg) {
                        if p.read().unwrap().match_str(&privmsg.message) {
                            let sqlt = rusqlite::Connection::open(TWITCH_DB_PATH).unwrap();
                            insert_message(&sqlt, privmsg);
                        }
                    }
                })
            }
        }
    });
    msg_sender
}

fn spawn_socket(emitter: AppEventEmitter) -> std::io::Result<()> {
    close_socket()?;
    let _ = tokio::spawn(async move {
        let ul = UnixListener::bind(SOCKET_PATH)?;
        while let Ok((mut stream, _)) = ul.accept().await {
            let mut buf = vec![];

            let _ = stream.read_to_end(&mut buf).await?;
            let action = serde_json::from_slice::<Action>(&buf)?;

            let (responder, res_receiver) = tokio::sync::oneshot::channel();
            let _ = emitter.send(AppEvent::ExternalAction { action, responder });
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
