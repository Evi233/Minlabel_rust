#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};

mod transcribe;
use transcribe::{Mode, Transcriber};

#[derive(Serialize, Deserialize, Default, Clone)]
struct LabelData {
    #[serde(default)]
    is_check: bool,
    #[serde(default)]
    lab: String,
    #[serde(default)]
    lab_without_tone: String,
    #[serde(default)]
    raw_text: String,
}

const ICON_PLAY: char = '\u{e037}';
const ICON_PAUSE: char = '\u{e034}';
const ICON_HEADPHONES: char = '\u{f01f}';
const ICON_PASTE: char = '\u{e14f}';

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_title("Minlabel"),
        ..Default::default()
    };
    eframe::run_native(
        "Minlabel",
        options,
        Box::new(|cc| Ok(Box::new(MinlabelApp::new(cc)))),
    )
}

struct MinlabelApp {
    folder: Option<PathBuf>,
    files: Vec<PathBuf>,
    current_index: usize,
    playing: bool,
    show_about: bool,
    status: String,
    player: Player,
    left_width: f32,
    size_col_width: f32,
    paste_text: String,
    annotation_text: String,
    mode: Mode,
    transcriber: Transcriber,
    labels: Vec<LabelData>,
}

impl MinlabelApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        Self {
            folder: None,
            files: Vec::new(),
            current_index: 0,
            playing: false,
            show_about: false,
            status: String::new(),
            player: Player::new(),
            left_width: 220.0,
            size_col_width: 80.0,
            paste_text: String::new(),
            annotation_text: String::new(),
            mode: Mode::Pinyin,
            transcriber: Transcriber::new(std::path::Path::new("assets/dict")),
            labels: Vec::new(),
        }
    }

    fn open_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.folder = Some(path.clone());
            self.files = Self::collect_files(&path);
            self.labels = vec![LabelData::default(); self.files.len()];
            self.current_index = 0;
            self.playing = false;
            self.player.stop();
            self.load_current_label();
            self.status = format!("Opened folder: {}", path.display());
        }
    }

    fn load_current_label(&mut self) {
        let Some(file) = self.current_file() else {
            return;
        };
        let stem = file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let json_path = file.with_file_name(format!("{stem}.json"));
        if let Ok(content) = std::fs::read_to_string(&json_path) {
            if let Ok(data) = serde_json::from_str::<LabelData>(&content) {
                self.labels[self.current_index] = data;
            }
        }
        self.paste_text = self.labels[self.current_index].raw_text.clone();
        self.annotation_text = self.labels[self.current_index].lab.clone();
    }

    fn save_current_label(&mut self) {
        let Some(file) = self.current_file() else {
            return;
        };
        let stem = file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let json_path = file.with_file_name(format!("{stem}.json"));
        let lab_path = file.with_file_name(format!("{stem}.lab"));

        let data = LabelData {
            is_check: true,
            lab: self.annotation_text.trim().to_string(),
            lab_without_tone: self.annotation_text.trim().to_string(),
            raw_text: self.paste_text.trim().to_string(),
        };
        let json = serde_json::to_string_pretty(&data).unwrap_or_default();
        if let Err(e) = std::fs::write(&json_path, json) {
            self.status = format!("Failed to write json: {e}");
            return;
        }
        if let Err(e) = std::fs::write(&lab_path, &data.lab) {
            self.status = format!("Failed to write lab: {e}");
            return;
        }
        self.labels[self.current_index] = data;
        self.status = format!("Saved {} and {}", json_path.display(), lab_path.display());
    }

    fn collect_files(dir: &PathBuf) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
                {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    }

    fn next_file(&mut self) {
        if self.files.is_empty() {
            return;
        }
        self.save_current_label();
        self.current_index = (self.current_index + 1) % self.files.len();
        self.playing = false;
        self.player.stop();
        self.load_current_label();
    }

    fn previous_file(&mut self) {
        if self.files.is_empty() {
            return;
        }
        self.save_current_label();
        self.current_index = (self.current_index + self.files.len() - 1) % self.files.len();
        self.playing = false;
        self.player.stop();
        self.load_current_label();
    }

    fn toggle_play(&mut self) {
        if self.files.is_empty() {
            self.status = "No files loaded. Open a folder first.".to_string();
            return;
        }
        if self.playing {
            self.player.pause();
            self.playing = false;
        } else {
            let file = self.files[self.current_index].clone();
            match self.player.play(&file) {
                Ok(()) => {
                    self.playing = true;
                    self.status = format!("Playing: {}", file.display());
                }
                Err(e) => {
                    self.playing = false;
                    self.status = format!("Failed to play: {e}");
                }
            }
        }
    }

    fn export(&mut self) {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            self.status = format!("Export to: {}", path.display());
        }
    }

    fn connect(&mut self) {
        self.status = "Connect: not implemented yet".to_string();
    }

    fn current_file(&self) -> Option<&PathBuf> {
        self.files.get(self.current_index)
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.open_folder();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::E)) {
            self.export();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown)) {
            self.next_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp)) {
            self.previous_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            self.toggle_play();
        }
    }
}

impl eframe::App for MinlabelApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ui.ctx());

        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Folder").clicked() {
                        self.open_folder();
                        ui.close();
                    }
                    if ui.button("Export").clicked() {
                        self.export();
                        ui.close();
                    }
                    if ui.button("Connect").clicked() {
                        self.connect();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Next File").clicked() {
                        self.next_file();
                        ui.close();
                    }
                    if ui.button("Previous File").clicked() {
                        self.previous_file();
                        ui.close();
                    }
                });
                ui.menu_button("Playback", |ui| {
                    if ui.button("play/stop").clicked() {
                        self.toggle_play();
                        ui.close();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.show_about = true;
                        ui.close();
                    }
                });
            });
        });

        egui::Panel::left("file_list")
            .resizable(false)
            .exact_size(self.left_width)
            .show(ui, |ui| {
                if self.files.is_empty() {
                    ui.label("No folder opened.");
                } else {
                    let row_height = ui.text_style_height(&egui::TextStyle::Body);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let name_col = (self.left_width - self.size_col_width - 8.0).max(40.0);
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [name_col, row_height],
                                egui::Label::new(egui::RichText::new("Name").strong()).truncate(),
                            );
                            let (sep_rect, _) = ui.allocate_exact_size(
                                egui::vec2(8.0, row_height),
                                egui::Sense::hover(),
                            );
                            let sep_response = ui.interact(
                                sep_rect,
                                egui::Id::new("col_separator"),
                                egui::Sense::drag(),
                            );
                            if sep_response.dragged() {
                                self.size_col_width = (self.size_col_width
                                    + sep_response.drag_delta().x)
                                    .clamp(40.0, 200.0);
                            }
                            if sep_response.hovered() || sep_response.dragged() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                            }
                            ui.add_sized(
                                [self.size_col_width, row_height],
                                egui::Label::new(egui::RichText::new("Size").strong()).truncate(),
                            );
                        });
                        let files = self.files.clone();
                        let labels = self.labels.clone();
                        for (i, file) in files.iter().enumerate() {
                            let selected = i == self.current_index;
                            let checked = labels.get(i).map(|l| l.is_check).unwrap_or(false);
                            let name = file
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| file.display().to_string());
                            let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                            let name_col =
                                (self.left_width - self.size_col_width - 8.0).max(40.0);
                            ui.horizontal(|ui| {
                                let text = if checked {
                                    egui::RichText::new(&name).weak()
                                } else {
                                    egui::RichText::new(&name)
                                };
                                let resp = ui.add_sized(
                                    [name_col, row_height],
                                    egui::Button::selectable(selected, text).truncate(),
                                );
                                if resp.clicked() {
                                    self.save_current_label();
                                    self.current_index = i;
                                    self.playing = false;
                                    self.player.stop();
                                    self.load_current_label();
                                }
                                let size_text = if checked {
                                    egui::RichText::new(format_size(size)).weak()
                                } else {
                                    egui::RichText::new(format_size(size))
                                };
                                ui.add_sized(
                                    [self.size_col_width, row_height],
                                    egui::Label::new(size_text).truncate(),
                                );
                            });
                        }
                    });
                }
            });

        let separator_rect = egui::Rect::from_min_max(
            egui::pos2(self.left_width, 0.0),
            egui::pos2(self.left_width + 4.0, ui.available_height()),
        );
        let sep_response = ui.interact(
            separator_rect,
            egui::Id::new("left_separator"),
            egui::Sense::drag(),
        );
        if sep_response.dragged() {
            let delta = sep_response.drag_delta();
            self.left_width = (self.left_width + delta.x).clamp(120.0, 500.0);
        }
        if sep_response.hovered() || sep_response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        ui.painter().vline(
            self.left_width + 2.0,
            ui.available_rect_before_wrap().y_range(),
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );

        egui::CentralPanel::default().show(ui, |ui| {
            self.player_ui(ui);
        });

        if self.show_about {
            egui::Window::new("About")
                .open(&mut self.show_about)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.heading("Minlabel");
                    ui.label("A minimal audio labeling tool.");
                    ui.label("Version 0.1.0");
                    ui.separator();
                    ui.label("Shortcuts:");
                    ui.label("  PageDown  - Next file");
                    ui.label("  PageUp    - Previous file");
                    ui.label("  F5        - Play / Stop");
                    ui.label("  Ctrl+O    - Open folder");
                    ui.label("  Ctrl+E    - Export");
                    ui.separator();
                    ui.hyperlink("https://github.com/Evi233/Minlabel_rust");
                });
        }
    }
}

impl MinlabelApp {
    fn apply_text(&mut self, append: bool) {
        let text = self.paste_text.trim().to_string();
        if text.is_empty() {
            self.status = "Input text is empty".to_string();
            return;
        }
        let transcribed = self.transcriber.transcribe(&text, &self.mode);
        if append && !self.annotation_text.is_empty() {
            self.annotation_text.push('\n');
        }
        self.annotation_text = transcribed;
        self.status = format!("Transcribed ({})", self.mode.to_string());
        self.save_current_label();
    }

    fn player_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        match self.current_file() {
            Some(file) => {
                ui.label(
                    egui::RichText::new(file.display().to_string())
                        .strong()
                        .size(16.0),
                );
            }
            None => {
                ui.label("No file selected. Open a folder first.");
                return;
            }
        }
        ui.add_space(8.0);

        let duration = self.player.duration_secs();
        let mut pos = self.player.position_secs();
        if self.playing && !self.player.is_playing() {
            self.playing = false;
            self.player.seek(0.0);
            pos = 0.0;
        }
        let slider = egui::Slider::new(&mut pos, 0.0..=duration.max(0.001))
            .show_value(false)
            .trailing_fill(true);
        ui.spacing_mut().slider_width = ui.available_width();
        if ui.add(slider).changed() {
            self.player.seek(pos);
        }
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let play_btn = egui::Button::new(egui::RichText::new(ICON_PLAY).size(18.0))
                .min_size(egui::vec2(36.0, 30.0));
            if ui.add(play_btn).clicked() {
                self.toggle_play();
            }

            let pause_btn = egui::Button::new(egui::RichText::new(ICON_PAUSE).size(18.0))
                .min_size(egui::vec2(36.0, 30.0));
            if ui.add(pause_btn).clicked() {
                self.player.pause();
                self.playing = false;
            }

            ui.menu_button(egui::RichText::new(ICON_HEADPHONES).size(18.0), |ui| {
                let devices = self.player.devices();
                if devices.is_empty() {
                    ui.label("No audio devices found.");
                } else {
                    for name in devices {
                        let selected = self.player.device.as_deref() == Some(name.as_str());
                        if ui.selectable_label(selected, &name).clicked() {
                            self.player.set_device(&name);
                            if self.playing {
                                if let Some(file) = self.current_file().cloned() {
                                    let _ = self.player.play(&file);
                                }
                            }
                            ui.close();
                        }
                    }
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} / {}",
                        format_time(pos),
                        format_time(duration)
                    ))
                    .monospace(),
                );
            });
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let paste_btn = egui::Button::new(egui::RichText::new(ICON_PASTE).size(18.0))
                .min_size(egui::vec2(30.0, 24.0));
            let resp = ui.add_sized(
                [ui.available_width() - 34.0, 24.0],
                egui::TextEdit::singleline(&mut self.paste_text).hint_text("Input text here"),
            );
            let submitted = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if resp.changed() {
                self.paste_text = self.paste_text.replace('\n', "");
            }
            if ui.add(paste_btn).clicked() {
                if let Some(text) = ui.ctx().input(|i| i.events.clone()).iter().find_map(|e| {
                    if let egui::Event::Paste(t) = e {
                        Some(t.clone())
                    } else {
                        None
                    }
                }) {
                    self.paste_text = text;
                }
            }
            if submitted {
                self.apply_text(false);
            }
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let btn_w = (ui.available_width() - 8.0) / 3.0;
            if ui
                .add_sized([btn_w, 26.0], egui::Button::new("Replace"))
                .clicked()
            {
                self.apply_text(false);
            }
            if ui
                .add_sized([btn_w, 26.0], egui::Button::new("Append"))
                .clicked()
            {
                self.apply_text(true);
            }
            egui::ComboBox::from_id_salt("mode_combo")
                .selected_text(self.mode.to_string())
                .width(btn_w)
                .show_ui(ui, |ui| {
                    for m in [Mode::Pinyin, Mode::Romaji, Mode::Cantonese] {
                        ui.selectable_value(&mut self.mode, m, m.to_string());
                    }
                });
        });

        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.add_sized(
                    [ui.available_width(), ui.available_height()],
                    egui::TextEdit::multiline(&mut self.annotation_text)
                        .hint_text("Annotation text"),
                );
            });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Progress:");
            let checked = self.labels.iter().filter(|l| l.is_check).count();
            let progress = if self.files.is_empty() {
                0.0
            } else {
                checked as f32 / self.files.len() as f32
            };
            let bar = egui::ProgressBar::new(progress).show_percentage();
            ui.add_sized([ui.available_width() - 50.0, 18.0], bar);
            ui.label(format!("{:.0}%", progress * 100.0));
        });
    }
}

// ---------------------------------------------------------------------------
// Audio player

struct Player {
    stream: Option<cpal::Stream>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
    position: Arc<AtomicU64>,
    playing: Arc<AtomicBool>,
    device: Option<String>,
    loaded_path: Option<PathBuf>,
}

impl Player {
    fn new() -> Self {
        Self {
            stream: None,
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 44100,
            channels: 1,
            position: Arc::new(AtomicU64::new(0)),
            playing: Arc::new(AtomicBool::new(false)),
            device: None,
            loaded_path: None,
        }
    }

    fn stop(&mut self) {
        self.playing.store(false, Ordering::SeqCst);
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        self.position.store(0, Ordering::SeqCst);
        self.samples.lock().unwrap().clear();
        self.loaded_path = None;
    }

    fn pause(&mut self) {
        self.playing.store(false, Ordering::SeqCst);
    }

    fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    fn play(&mut self, path: &PathBuf) -> Result<(), String> {
        self.playing.store(false, Ordering::SeqCst);
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        let same_file = self.loaded_path.as_deref() == Some(path.as_path());
        if !same_file {
            let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
            let spec = reader.spec();
            let file_sample_rate = spec.sample_rate;
            let channels = spec.channels;
            let samples: Vec<f32> = match spec.sample_format {
                hound::SampleFormat::Float => reader
                    .samples::<f32>()
                    .map(|s| s.unwrap_or(0.0))
                    .collect(),
                hound::SampleFormat::Int => {
                    let bits = spec.bits_per_sample;
                    let max = (1i64 << (bits - 1)) as f32;
                    reader
                        .samples::<i32>()
                        .map(|s| s.unwrap_or(0) as f32 / max)
                        .collect()
                }
            };
            if samples.is_empty() {
                return Err("No audio samples".to_string());
            }

            let host = cpal::default_host();
            let device = host
                .default_output_device()
                .ok_or_else(|| "No output device".to_string())?;
            self.device = Some(device.to_string());
            let config = device
                .default_output_config()
                .map_err(|e| e.to_string())?;
            let device_sample_rate = config.sample_rate();
            let device_channels = config.channels() as usize;

            let samples = resample_frames(
                &samples,
                channels as usize,
                file_sample_rate,
                device_sample_rate,
            );
            let samples = convert_channels(&samples, channels as usize, device_channels);

            *self.samples.lock().unwrap() = samples;
            self.sample_rate = device_sample_rate;
            self.channels = device_channels as u16;
            self.position.store(0, Ordering::SeqCst);
            self.loaded_path = Some(path.clone());
        }

        self.playing.store(true, Ordering::SeqCst);

        let samples = Arc::clone(&self.samples);
        let position = Arc::clone(&self.position);
        let playing = Arc::clone(&self.playing);

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No output device".to_string())?;
        self.device = Some(device.to_string());
        let config = device
            .default_output_config()
            .map_err(|e| e.to_string())?;
        let stream = device
            .build_output_stream(
                config.into(),
                move |data: &mut [f32], _| {
                    let samples = samples.lock().unwrap();
                    let pos = position.load(Ordering::SeqCst) as usize;
                    if !playing.load(Ordering::SeqCst) {
                        for s in data.iter_mut() {
                            *s = 0.0;
                        }
                        return;
                    }
                    for (i, s) in data.iter_mut().enumerate() {
                        let idx = pos + i;
                        if idx < samples.len() {
                            *s = samples[idx];
                        } else {
                            *s = 0.0;
                        }
                    }
                    let new_pos = pos + data.len();
                    if new_pos >= samples.len() {
                        playing.store(false, Ordering::SeqCst);
                    }
                    position.store(new_pos as u64, Ordering::SeqCst);
                },
                |err| eprintln!("Audio stream error: {err}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(())
    }

    fn duration_secs(&self) -> f64 {
        let n = self.samples.lock().unwrap().len();
        if n == 0 || self.sample_rate == 0 {
            0.0
        } else {
            n as f64 / self.sample_rate as f64 / self.channels as f64
        }
    }

    fn position_secs(&self) -> f64 {
        let pos = self.position.load(Ordering::SeqCst) as f64;
        if self.sample_rate == 0 {
            0.0
        } else {
            pos / self.sample_rate as f64 / self.channels as f64
        }
    }

    fn seek(&mut self, secs: f64) {
        let idx = (secs * self.sample_rate as f64 * self.channels as f64) as u64;
        self.position.store(idx, Ordering::SeqCst);
    }

    fn devices(&self) -> Vec<String> {
        let host = cpal::default_host();
        host.output_devices()
            .map(|devs| devs.map(|d| d.to_string()).collect::<Vec<_>>())
            .unwrap_or_default()
    }

    fn set_device(&mut self, name: &str) {
        self.device = Some(name.to_string());
    }
}

fn format_time(secs: f64) -> String {
    let secs = secs.max(0.0) as u64;
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

fn resample_frames(samples: &[f32], channels: usize, from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let frames = samples.len() / channels;
    let ratio = to_rate as f64 / from_rate as f64;
    let out_frames = (frames as f64 * ratio).ceil() as usize;
    let mut out = vec![0.0f32; out_frames * channels];
    for f in 0..out_frames {
        let src_pos = f as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let next = (idx + 1).min(frames - 1);
        for c in 0..channels {
            let a = samples[idx * channels + c];
            let b = samples[next * channels + c];
            out[f * channels + c] = a * (1.0 - frac) + b * frac;
        }
    }
    out
}

fn convert_channels(samples: &[f32], from: usize, to: usize) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let frames = samples.len() / from;
    let mut out = vec![0.0f32; frames * to];
    for f in 0..frames {
        for c in 0..to {
            let src = if from == 1 {
                samples[f]
            } else {
                samples[f * from + (c % from)]
            };
            out[f * to + c] = src;
        }
    }
    out
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "NotoSansSC".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/NotoSansSC-Regular.otf"
        ))),
    );
    fonts.font_data.insert(
        "NotoEmoji".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/NotoEmoji-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "MaterialIcons".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/MaterialIcons-Regular.ttf"
        ))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        list.insert(0, "NotoSansSC".to_owned());
        list.push("NotoEmoji".to_owned());
        list.push("MaterialIcons".to_owned());
    }
    ctx.set_fonts(fonts);
}
