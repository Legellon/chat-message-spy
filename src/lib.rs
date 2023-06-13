#![feature(lazy_cell)]

use crate::match_pattern::MatchPattern;
use crate::protocol::{Action, ActionRes};
use fnv::FnvHashMap;
use std::sync::{Arc, RwLock};

pub mod match_pattern;
pub mod network;
pub mod protocol;
pub mod storage;
pub mod twitch;

pub const SOCKET_PATH: &str = "/tmp/chatspy.socket";
pub const TWITCH_DB_PATH: &str = "./twitch_storage.sqlite";

#[derive(Debug)]
pub enum TwitchEvent {
    Message(String),
}

#[derive(Debug)]
pub enum AppEvent {
    Twitch(TwitchEvent),
    ExternalAction {
        action: Action,
        responder: tokio::sync::oneshot::Sender<ActionRes>,
    },
    Error,
}

pub type LockedPattern = Locked<MatchPattern>;
pub type AppEventEmitter = crossbeam::channel::Sender<AppEvent>;

type Locked<T> = Arc<RwLock<T>>;

#[derive(Debug)]
pub struct PatternStorage {
    patterns: RwLock<FnvHashMap<String, LockedPattern>>,
    active_pattern: RwLock<Option<LockedPattern>>,
}

impl PatternStorage {
    pub fn new() -> Self {
        PatternStorage {
            patterns: RwLock::new(FnvHashMap::default()),
            active_pattern: RwLock::new(None),
        }
    }

    pub fn add(&self, n: String, p: MatchPattern, new_default: bool) -> Result<(), ()> {
        let mut patterns_lock = self.patterns.write().unwrap();
        if !patterns_lock.contains_key(&n) {
            let p = Arc::new(RwLock::new(p));
            let mut default_lock = self.active_pattern.write().unwrap();
            if default_lock.is_none() || new_default {
                *default_lock = Some(p.clone());
            }
            patterns_lock.insert(n, p);
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn default_pattern(&self) -> &RwLock<Option<LockedPattern>> {
        &self.active_pattern
    }
}

impl Default for PatternStorage {
    fn default() -> Self {
        PatternStorage::new()
    }
}
