mod match_pattern;
mod network;
mod storage;
mod twitch;

use crate::storage::{create_message_table, create_token_table, get_stored_users, TwitchToken};
use crate::twitch::spawn_twitch_irc_connection;
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

const CHANNELS: [&str; 14] = [
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
];

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let sqlite_conn = rusqlite::Connection::open_in_memory().unwrap();

    create_token_table(&sqlite_conn);
    create_message_table(&sqlite_conn);

    let mut tokens = get_stored_users(&sqlite_conn);

    if tokens.is_empty() {
        // let task = tokio::spawn(async move {
        //     println!("{}", twitch_auth_uri(RESERVED_PORTS[0]));
        //     let token = get_twitch_token(RESERVED_PORTS[0]).await;
        //     let login = validate_twitch_token(token.as_str()).await;
        //     (token, login)
        // });
        //
        // match task.await {
        //     Ok((token, Some(login))) => insert_twitch_token(&sqlite_conn, &token, &login),
        //     Ok((_, None)) => panic!("twitch token isn't valid"),
        //     Err(e) => panic!("{}", e),
        // }
        //
        // tokens = get_stored_users(&sqlite_conn);
        tokens.push(TwitchToken {
            id: 0,
            token: "".to_string(),
            login: "justinfan2281337322".to_string(),
        });
    }

    let _target_token = if tokens.len() == 1 {
        tokens[0].clone()
    } else {
        unimplemented!("ERROR: can't choose from many tokens yet");
    };

    let (_tx, mut rx) = spawn_twitch_irc_connection(&CHANNELS);

    loop {
        match rx.recv().await {
            Some(s) => print!("{}", s),
            None => panic!("ERROR: something went wrong with receiver"),
        }
    }
}
