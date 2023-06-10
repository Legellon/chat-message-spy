use crate::twitch::UserMessage;
use rusqlite::{params_from_iter, Connection, Error};
use serde::{Deserialize, Serialize};

fn insert_twitch_message(_conn: &Connection, _message: TwitchMessage) {}

#[derive(Debug, Clone)]
pub struct TwitchToken {
    pub id: Option<u64>,
    pub token: Option<String>,
    pub login: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct TwitchMessage {
    pub id: u64,
    pub author: String,
    pub message: String,
    pub channel: String,
    pub time: String,
}

pub fn run_init_migration(conn: &Connection) {
    create_token_table(conn);
    create_messages_table(conn);
}

pub fn create_token_table(conn: &Connection) {
    if let Err(e) = conn.execute(
        "CREATE TABLE token (id INTEGER PRIMARY KEY, token TEXT NOT NULL, login TEXT NOT NULL)",
        (),
    ) {
        ignore_table_exists_error(e);
    }
}

pub fn insert_twitch_token(conn: &Connection, token: &str, login: &str) {
    conn.execute(
        "INSERT INTO token (token, login) VALUES (?1, ?2)",
        (token, login),
    )
    .unwrap();
}

pub fn get_stored_users(db_conn: &Connection) -> Vec<TwitchToken> {
    match db_conn.prepare("SELECT * FROM token") {
        Ok(mut s) => s
            .query_map([], |row| {
                Ok(TwitchToken {
                    id: Some(row.get(0).unwrap()),
                    token: Some(row.get(1).unwrap()),
                    login: Some(row.get(2).unwrap()),
                })
            })
            .unwrap()
            .map(|u| u.unwrap())
            .collect(),
        Err(e) => panic!("ERROR: failed to select from 'token': {}", e),
    }
}

pub fn create_messages_table(conn: &Connection) {
    if let Err(e) = conn.execute(
        "CREATE TABLE messages (\
        id      INTEGER PRIMARY KEY,\
        author  TEXT NOT NULL,\
        message TEXT NOT NULL,\
        channel TEXT NOT NULL,\
        time    TIMESTAMP DATETIME DEFAULT CURRENT_TIMESTAMP\
        )",
        (),
    ) {
        ignore_table_exists_error(e);
    }
}

pub fn insert_message(conn: &Connection, privmsg: UserMessage) {
    conn.execute(
        "INSERT INTO messages (author, message, channel) VALUES (?1, ?2, ?3)",
        (privmsg.author, privmsg.message, privmsg.channel),
    )
    .unwrap();
}

pub fn get_messages(
    conn: &Connection,
    author: Option<String>,
    channel: Option<String>,
) -> Vec<TwitchMessage> {
    let (sql, params) = match (author, channel) {
        (None, None) => ("SELECT * FROM messages", vec![]),
        (Some(a), Some(ch)) => (
            "SELECT * FROM messages WHERE author=?1 AND channel=?2",
            vec![a, ch],
        ),
        (_, Some(ch)) => ("SELECT * FROM messages WHERE channel=?1", vec![ch]),
        (Some(a), _) => ("SELECT * FROM messages WHERE author=?1", vec![a]),
    };

    match conn.prepare(sql) {
        Ok(mut s) => s
            .query_map(params_from_iter(params), |row| {
                Ok(TwitchMessage {
                    id: row.get(0).unwrap(),
                    author: row.get(1).unwrap(),
                    message: row.get(2).unwrap(),
                    channel: row.get(3).unwrap(),
                    time: row.get(4).unwrap(),
                })
            })
            .unwrap()
            .map(|u| u.unwrap())
            .collect(),
        Err(e) => panic!("ERROR: failed to select from 'token': {}", e),
    }
}

fn ignore_table_exists_error(e: Error) {
    match e {
        // Error has values which indicate that this is 'table already exists' error
        Error::SqlInputError {
            error:
                rusqlite::ffi::Error {
                    code: rusqlite::ffi::ErrorCode::Unknown,
                    extended_code: 1,
                },
            ..
        } => {}
        // Otherwise, panic
        e => panic!("{:?}", e),
    };
}
