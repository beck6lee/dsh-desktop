use std::sync::Mutex;
use std::process::Child;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub state: String,
    pub port: Option<u16>,
    pub error: Option<String>,
    pub pid: Option<u32>,
    pub owned: bool,
}

pub struct ServerManager {
    child: Mutex<Option<Child>>,
    owned: AtomicBool,
    port: Mutex<u16>,
    state: Mutex<ServerState>,
}

enum ServerState {
    Starting,
    Ready { port: u16 },
    Reused { port: u16 },
    Error(String),
}

impl ServerManager {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            owned: AtomicBool::new(false),
            port: Mutex::new(0),
            state: Mutex::new(ServerState::Starting),
        }
    }
    pub fn start(&self) {}
    pub fn stop(&self) {}
    pub fn restart(&self) {}
    pub fn status(&self) -> StatusSnapshot {
        StatusSnapshot {
            state: "starting".to_string(),
            port: None,
            error: None,
            pid: None,
            owned: false,
        }
    }
}
