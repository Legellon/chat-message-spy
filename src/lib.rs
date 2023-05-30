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

type Locked<T> = Arc<RwLock<T>>;
pub type LockedPattern = Locked<MatchPattern>;
pub type LockedPatternMap = Locked<FnvHashMap<String, LockedPattern>>;
pub type LockedDefaultPattern = Locked<Option<LockedPattern>>;

#[derive(Debug)]
pub struct PatternStorage {
    patterns: LockedPatternMap,
    default_pattern: LockedDefaultPattern,
}

impl PatternStorage {
    pub fn new() -> Self {
        PatternStorage {
            patterns: Arc::new(RwLock::new(FnvHashMap::default())),
            default_pattern: Arc::new(RwLock::new(None)),
        }
    }

    pub fn add(&self, n: String, p: MatchPattern, new_default: bool) -> Result<(), ()> {
        let mut patterns_lock = self.patterns.write().unwrap();
        if !patterns_lock.contains_key(&n) {
            let p = Arc::new(RwLock::new(p));
            let mut default_lock = self.default_pattern.write().unwrap();
            if default_lock.is_none() || new_default {
                *default_lock = Some(p.clone());
            }
            patterns_lock.insert(n, p);
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn default_pattern(&self) -> LockedDefaultPattern {
        self.default_pattern.clone()
    }
}

impl Default for PatternStorage {
    fn default() -> Self {
        PatternStorage::new()
    }
}
