use std::error::Error;
use crate::match_pattern::MatchPattern;
use crate::protocol::{Action, ActionRes};
use fnv::FnvHashMap;
use std::sync::{Arc, LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard};

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
type LockedPattern = Locked<MatchPattern>;
type LockedPatternMap = Locked<FnvHashMap<String, LockedPattern>>;
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
        // if let Some(p) = &self.default_pattern.read().unwrap() {
        //     Ok()
        // } else {
        //     Err(())
        // }
        // let lock = self.default_pattern.read().unwrap();
        // if let Some(p) = lock.clone() {
        //     Ok(p.clone())
        // } else {
        //     Err(())
        // }
        self.default_pattern.clone()
    }
}
