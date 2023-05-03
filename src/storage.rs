use rusqlite::{Connection, Error};

fn insert_twitch_message(_conn: &Connection, _message: TwitchMessage) {}

#[derive(Debug, Clone)]
pub struct TwitchToken {
    pub id: u64,
    pub token: String,
    pub login: String,
}

#[derive(Debug)]
pub struct TwitchMessage {
    pub id: u64,
    pub author: String,
    pub content: String,
    pub chat: String,
    pub time: String,
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
                    id: row.get(0).unwrap(),
                    token: row.get(1).unwrap(),
                    login: row.get(2).unwrap(),
                })
            })
            .unwrap()
            .map(|u| u.unwrap())
            .collect(),
        Err(e) => panic!("ERROR: failed to select from 'token': {}", e),
    }
}

pub fn create_message_table(conn: &Connection) {
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
        ignore_table_exists_error(e);
    }
}

fn ignore_table_exists_error(e: Error) {
    match e {
        //Error has values which indicates that query failed with an error "Table already exists", then ignore it
        Error::SqlInputError {
            error:
                rusqlite::ffi::Error {
                    code: rusqlite::ffi::ErrorCode::Unknown,
                    extended_code: 1,
                },
            ..
        } => {}
        //Otherwise, panic
        e => panic!("{:?}", e),
    };
}
