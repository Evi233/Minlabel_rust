use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConnectInfo {
    pub server: String,
    pub port: u16,
    pub http_port: u16,
    pub username: String,
}

impl Default for ConnectInfo {
    fn default() -> Self {
        Self {
            server: "127.0.0.1".to_string(),
            port: 9000,
            http_port: 8080,
            username: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Hello { username: String },
    FileList { files: Vec<String> },
    Lock { file: String, username: String },
    Unlock { file: String },
    Label { file: String, label: LabelData },
    Error { message: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    Hello { username: String },
    RequestLock { file: String },
    ReleaseLock { file: String },
    SubmitLabel { file: String, label: LabelData },
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LabelData {
    #[serde(default)]
    pub is_check: bool,
    #[serde(default)]
    pub lab: String,
    #[serde(default)]
    pub lab_without_tone: String,
    #[serde(default)]
    pub raw_text: String,
}

#[derive(Clone, Debug)]
pub enum NetEvent {
    Connected,
    Disconnected(String),
    FileList(Vec<String>),
    Locked { file: String, username: String },
    Unlocked { file: String },
    LabelUpdated { file: String, label: LabelData },
    Error(String),
}

pub struct NetClient {
    tx: Sender<ClientMsg>,
    pub events: Arc<Mutex<Receiver<NetEvent>>>,
    pub connected: Arc<Mutex<bool>>,
    pub username: String,
}

impl NetClient {
    pub fn connect(info: &ConnectInfo) -> Result<Self, String> {
        let (msg_tx, msg_rx) = channel::<ClientMsg>();
        let (evt_tx, evt_rx) = channel::<NetEvent>();
        let connected = Arc::new(Mutex::new(false));
        let connected_clone = Arc::clone(&connected);

        let url = format!("ws://{}:{}/ws", info.server, info.port);
        let username = info.username.clone();
        let hello = ClientMsg::Hello {
            username: username.clone(),
        };

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let rt = match rt {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = evt_tx.send(NetEvent::Error(format!("Runtime: {e}")));
                    return;
                }
            };
            rt.block_on(async move {
                let (mut ws, _) = match tokio_tungstenite::connect_async(&url).await {
                    Ok(pair) => pair,
                    Err(e) => {
                        let _ = evt_tx.send(NetEvent::Error(format!("Connect: {e}")));
                        return;
                    }
                };
                *connected_clone.lock().unwrap() = true;
                let _ = evt_tx.send(NetEvent::Connected);

                let _ = ws.send(tokio_tungstenite::tungstenite::Message::Text(
                    serde_json::to_string(&hello).unwrap_or_default(),
                ));

                loop {
                    tokio::select! {
                        msg = msg_rx.recv() => {
                            let Some(msg) = msg else { break };
                            let text = serde_json::to_string(&msg).unwrap_or_default();
                            if ws.send(tokio_tungstenite::tungstenite::Message::Text(text)).await.is_err() {
                                break;
                            }
                        }
                        incoming = ws.next() => {
                            match incoming {
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                                    if let Ok(msg) = serde_json::from_str::<ServerMsg>(&text) {
                                        let evt = match msg {
                                            ServerMsg::Hello { username } => NetEvent::Locked { file: String::new(), username },
                                            ServerMsg::FileList { files } => NetEvent::FileList(files),
                                            ServerMsg::Lock { file, username } => NetEvent::Locked { file, username },
                                            ServerMsg::Unlock { file } => NetEvent::Unlocked { file },
                                            ServerMsg::Label { file, label } => NetEvent::LabelUpdated { file, label },
                                            ServerMsg::Error { message } => NetEvent::Error(message),
                                        };
                                        let _ = evt_tx.send(evt);
                                    }
                                }
                                Some(Ok(_)) => {}
                                Some(Err(e)) => {
                                    let _ = evt_tx.send(NetEvent::Disconnected(e.to_string()));
                                    break;
                                }
                                None => {
                                    let _ = evt_tx.send(NetEvent::Disconnected("Connection closed".to_string()));
                                    break;
                                }
                            }
                        }
                    }
                }
                *connected_clone.lock().unwrap() = false;
            });
        });

        Ok(Self {
            tx: msg_tx,
            events: Arc::new(Mutex::new(evt_rx)),
            connected,
            username,
        })
    }

    pub fn send(&self, msg: ClientMsg) {
        let _ = self.tx.send(msg);
    }

    pub fn request_lock(&self, file: &str) {
        self.send(ClientMsg::RequestLock {
            file: file.to_string(),
        });
    }

    pub fn release_lock(&self, file: &str) {
        self.send(ClientMsg::ReleaseLock {
            file: file.to_string(),
        });
    }

    pub fn submit_label(&self, file: &str, label: &LabelData) {
        self.send(ClientMsg::SubmitLabel {
            file: file.to_string(),
            label: label.clone(),
        });
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }
}

pub fn fetch_file_list(http_port: u16) -> Result<Vec<String>, String> {
    let url = format!("http://127.0.0.1:{http_port}/files");
    let resp = reqwest::blocking::get(&url).map_err(|e| e.to_string())?;
    let list: Vec<String> = resp.json().map_err(|e| e.to_string())?;
    Ok(list)
}

pub fn fetch_label(http_port: u16, file: &str) -> Result<Option<LabelData>, String> {
    let url = format!("http://127.0.0.1:{http_port}/label/{}", urlencode(file));
    let resp = reqwest::blocking::get(&url).map_err(|e| e.to_string())?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let label: LabelData = resp.json().map_err(|e| e.to_string())?;
    Ok(Some(label))
}

pub fn fetch_wav(http_port: u16, file: &str, dest: &std::path::Path) -> Result<(), String> {
    let url = format!("http://127.0.0.1:{http_port}/wav/{}", urlencode(file));
    let resp = reqwest::blocking::get(&url).map_err(|e| e.to_string())?;
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    std::fs::write(dest, bytes).map_err(|e| e.to_string())
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub type LockMap = HashMap<String, String>;
