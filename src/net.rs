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
pub struct FileInfo {
    pub id: u32,
    pub name: String,
    pub status: String,
    pub annotated_by: Option<String>,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    Claim { file_id: u32 },
    Release { file_id: u32 },
    Annotate {
        file_id: u32,
        data: LabelData,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Presence { user: String, file_id: u32 },
    Release { user: String, file_id: u32 },
    Annotated {
        user: String,
        file_id: u32,
        data: LabelData,
    },
    Progress { done: u32, total: u32 },
}

#[derive(Clone, Debug)]
pub enum NetEvent {
    Connected,
    Disconnected(String),
    Presence { user: String, file_id: u32 },
    Released { user: String, file_id: u32 },
    Annotated {
        user: String,
        file_id: u32,
        data: LabelData,
    },
    Progress { done: u32, total: u32 },
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

        let url = format!(
            "ws://{}:{}/ws?user={}",
            info.server,
            info.port,
            urlencode(&info.username)
        );
        let username = info.username.clone();

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
                                            ServerMsg::Presence { user, file_id } => NetEvent::Presence { user, file_id },
                                            ServerMsg::Release { user, file_id } => NetEvent::Released { user, file_id },
                                            ServerMsg::Annotated { user, file_id, data } => NetEvent::Annotated { user, file_id, data },
                                            ServerMsg::Progress { done, total } => NetEvent::Progress { done, total },
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

    pub fn claim(&self, file_id: u32) {
        self.send(ClientMsg::Claim { file_id });
    }

    pub fn release(&self, file_id: u32) {
        self.send(ClientMsg::Release { file_id });
    }

    pub fn annotate(&self, file_id: u32, data: &LabelData) {
        self.send(ClientMsg::Annotate {
            file_id,
            data: data.clone(),
        });
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }
}

pub fn fetch_file_list(server: &str, http_port: u16) -> Result<Vec<FileInfo>, String> {
    let url = format!("http://{server}:{http_port}/api/files");
    let resp = reqwest::blocking::get(&url).map_err(|e| e.to_string())?;
    let list: Vec<FileInfo> = resp.json().map_err(|e| e.to_string())?;
    Ok(list)
}

pub fn fetch_annotation(
    server: &str,
    http_port: u16,
    file_id: u32,
) -> Result<Option<LabelData>, String> {
    let url = format!("http://{server}:{http_port}/api/annotations/{file_id}");
    let resp = reqwest::blocking::get(&url).map_err(|e| e.to_string())?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let label: LabelData = resp.json().map_err(|e| e.to_string())?;
    Ok(Some(label))
}

pub fn fetch_audio(
    server: &str,
    http_port: u16,
    file_id: u32,
    dest: &std::path::Path,
) -> Result<(), String> {
    let url = format!("http://{server}:{http_port}/api/files/{file_id}/audio");
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

pub type LockMap = HashMap<u32, String>;
