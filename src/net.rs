use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConnectInfo {
    pub server: String,
    pub port: u16,
    pub http_port: u16,
    pub username: String,
    pub room: String,
}

impl Default for ConnectInfo {
    fn default() -> Self {
        Self {
            server: "127.0.0.1".to_string(),
            // The server serves HTTP and WebSocket on the same port
            // (MINLABEL_ADDR, default 8080), so both default to it.
            port: 8080,
            http_port: 8080,
            username: String::new(),
            room: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileInfo {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub uploaded: bool,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
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
    Claim {
        file_id: u32,
    },
    Release {
        file_id: u32,
    },
    Annotate {
        file_id: u32,
        data: LabelData,
    },
    /// Ask the room to make this file's audio available; the owning client
    /// will be asked to upload it on demand.
    RequestFile {
        file_id: u32,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Presence {
        user: String,
        file_id: u32,
    },
    Release {
        user: String,
        file_id: u32,
    },
    Annotated {
        user: String,
        file_id: u32,
        data: LabelData,
    },
    Progress {
        done: u32,
        total: u32,
    },
    /// Another member asked for a file this client owns: upload it now.
    FileRequested {
        file_id: u32,
    },
    /// A file's bytes have been uploaded to the server.
    FileUploaded {
        file_id: u32,
    },
    /// The requested file is already on the server: download it.
    FileReady {
        file_id: u32,
    },
    /// The file's owner is not connected, so it cannot be served right now.
    FileUnavailable {
        file_id: u32,
    },
}

#[derive(Clone, Debug)]
pub enum NetEvent {
    Connected,
    Disconnected(String),
    Presence { user: String, file_id: u32 },
    Released { file_id: u32 },
    Annotated { file_id: u32, data: LabelData },
    Progress { done: u32, total: u32 },
    FileRequested { file_id: u32 },
    FileUploaded { file_id: u32 },
    FileReady { file_id: u32 },
    FileUnavailable { file_id: u32 },
    Error(String),
}

/// Results of background upload/download work, consumed on the UI thread.
#[derive(Debug)]
pub enum IoEvent {
    RoomCreated {
        code: String,
        client: NetClient,
        files: Vec<FileInfo>,
    },
    RoomJoined {
        client: NetClient,
        files: Vec<FileInfo>,
    },
    RoomFailed(String),
    /// Metadata of newly registered files (no audio bytes uploaded yet).
    FilesRegistered(Vec<FileInfo>),
    DownloadDone {
        file_id: u32,
        path: PathBuf,
    },
    DownloadFailed {
        file_id: u32,
        msg: String,
    },
    UploadDone {
        file_id: u32,
        ok: bool,
        msg: String,
    },
}

#[derive(Debug)]
pub struct NetClient {
    tx: mpsc::Sender<ClientMsg>,
    pub events: Arc<Mutex<Receiver<NetEvent>>>,
    pub username: String,
}

impl NetClient {
    pub fn connect(info: &ConnectInfo) -> Result<Self, String> {
        let (msg_tx, mut msg_rx) = mpsc::channel::<ClientMsg>(64);
        let (evt_tx, evt_rx) = channel::<NetEvent>();

        let url = format!(
            "ws://{}:{}/ws?user={}&room={}",
            info.server,
            info.port,
            urlencode(&info.username),
            urlencode(&info.room)
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
                let (mut ws, _) = match tokio::time::timeout(
                    HTTP_TIMEOUT,
                    tokio_tungstenite::connect_async(&url),
                )
                .await
                {
                    Ok(Ok(pair)) => pair,
                    Ok(Err(e)) => {
                        let _ = evt_tx.send(NetEvent::Error(format!("Connect: {e}")));
                        return;
                    }
                    Err(_) => {
                        let _ = evt_tx.send(NetEvent::Error("Connect: timed out".to_string()));
                        return;
                    }
                };
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
                                            ServerMsg::Release { user: _, file_id } => NetEvent::Released { file_id },
                                            ServerMsg::Annotated { user: _, file_id, data } => NetEvent::Annotated { file_id, data },
                                            ServerMsg::Progress { done, total } => NetEvent::Progress { done, total },
                                            ServerMsg::FileRequested { file_id } => NetEvent::FileRequested { file_id },
                                            ServerMsg::FileUploaded { file_id } => NetEvent::FileUploaded { file_id },
                                            ServerMsg::FileReady { file_id } => NetEvent::FileReady { file_id },
                                            ServerMsg::FileUnavailable { file_id } => NetEvent::FileUnavailable { file_id },
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
            });
        });

        Ok(Self {
            tx: msg_tx,
            events: Arc::new(Mutex::new(evt_rx)),
            username,
        })
    }

    pub fn send(&self, msg: ClientMsg) {
        let _ = self.tx.blocking_send(msg);
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

    pub fn request_file(&self, file_id: u32) {
        self.send(ClientMsg::RequestFile { file_id });
    }
}

// ---------------------------------------------------------------------------
// Blocking HTTP helpers (run on background threads)

const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .unwrap_or_default()
}

/// Create a room; returns the 6-character room code.
pub fn create_room(server: &str, http_port: u16, user: &str) -> Result<String, String> {
    let url = format!("http://{server}:{http_port}/api/rooms");
    let resp = http_client()
        .post(&url)
        .json(&serde_json::json!({ "user": user }))
        .send()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    v["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "no room id".to_string())
}

/// Register file metadata (name/size) without uploading the audio bytes.
/// Returns the registered files with their server ids.
pub fn register_files(
    server: &str,
    http_port: u16,
    room: &str,
    user: &str,
    files: &[(String, u64)],
) -> Result<Vec<FileInfo>, String> {
    let url = format!("http://{server}:{http_port}/api/rooms/{room}/files");
    let list: Vec<serde_json::Value> = files
        .iter()
        .map(|(name, size)| serde_json::json!({ "name": name, "size": size }))
        .collect();
    let resp = http_client()
        .post(&url)
        .json(&serde_json::json!({ "user": user, "files": list }))
        .send()
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    v["files"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|f| serde_json::from_value(f).map_err(|e| e.to_string()))
        .collect()
}

pub fn fetch_room_files(server: &str, http_port: u16, room: &str) -> Result<Vec<FileInfo>, String> {
    let url = format!("http://{server}:{http_port}/api/rooms/{room}/files");
    let resp = http_client().get(&url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("list files failed: {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let files: Vec<FileInfo> =
        serde_json::from_value(v.get("files").cloned().unwrap_or(serde_json::json!([])))
            .map_err(|e| e.to_string())?;
    Ok(files)
}

/// Upload a file's audio bytes to the server (called when another room
/// member requested the file). The matching .lab / .json sidecar files are
/// uploaded too when they exist next to the audio.
pub fn upload_audio(
    server: &str,
    http_port: u16,
    room: &str,
    user: &str,
    file_id: u32,
    path: &std::path::Path,
) -> Result<(), String> {
    let url = format!("http://{server}:{http_port}/api/rooms/{room}/files/{file_id}/audio");
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "audio.bin".to_string());
    let mut form = reqwest::blocking::multipart::Form::new()
        .text("user", user.to_string())
        .part(
            "file",
            reqwest::blocking::multipart::Part::file(path)
                .map_err(|e| e.to_string())?
                .file_name(name),
        );
    if let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().to_string()) {
        for ext in ["lab", "json"] {
            let sidecar = path.with_file_name(format!("{stem}.{ext}"));
            if let Ok(text) = std::fs::read_to_string(&sidecar) {
                form = form.text(ext, text);
            }
        }
    }
    let resp = http_client()
        .post(&url)
        .multipart(form)
        .send()
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("upload failed: {}", resp.status()))
    }
}

pub fn fetch_audio(
    server: &str,
    http_port: u16,
    file_id: u32,
    dest: &std::path::Path,
) -> Result<(), String> {
    let url = format!("http://{server}:{http_port}/api/files/{file_id}/audio");
    let resp = http_client().get(&url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("download failed: {}", resp.status()));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(dest, bytes).map_err(|e| e.to_string())
}

/// Download a .lab / .json sidecar next to the audio. Returns Ok(false) when
/// the server has no such sidecar for this file.
pub fn fetch_sidecar(
    server: &str,
    http_port: u16,
    file_id: u32,
    ext: &str,
    dest: &std::path::Path,
) -> Result<bool, String> {
    let url = format!("http://{server}:{http_port}/api/files/{file_id}/{ext}");
    let resp = http_client().get(&url).send().map_err(|e| e.to_string())?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }
    if !resp.status().is_success() {
        return Err(format!("sidecar download failed: {}", resp.status()));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(dest, bytes).map_err(|e| e.to_string())?;
    Ok(true)
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
