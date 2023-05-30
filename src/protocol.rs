use crate::match_pattern::MatchMode;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

type Channels = Vec<String>;
type RawPattern = (Vec<String>, MatchMode);

#[derive(Deserialize, Serialize, Debug)]
pub enum PartAction {
    All,
    One(String),
    Many(Channels),
}

#[derive(Deserialize, Serialize, Debug)]
pub enum JoinAction {
    One(String),
    Many(Channels),
}

#[derive(Deserialize, Serialize, Debug)]
pub enum StartAction {
    Simple,
    Prejoin(Channels),
}

#[derive(Deserialize, Serialize, Debug)]
pub enum AddAction {
    Pattern {
        name: String,
        raw_pattern: RawPattern,
        default: bool,
    },
}

#[derive(Deserialize, Serialize, Debug)]
pub enum TwitchAction {
    Join(JoinAction),
    Start(StartAction),
    Part(PartAction),
}

#[derive(Deserialize, Serialize, Debug)]
pub enum Action {
    Twitch(TwitchAction),
    Add(AddAction),
    Kill,
}

#[derive(Deserialize, Serialize, Debug)]
pub enum Error {
    JoinFail { channel: String },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::JoinFail { channel } => write!(f, "failed to join to channel: {}", channel),
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub enum FailureLevel {
    Critical,
    Uncritical,
}

#[derive(Deserialize, Serialize, Debug)]
pub enum ActionRes {
    Failure {
        errors: Vec<Error>,
        level: FailureLevel,
    },
    Success,
}
