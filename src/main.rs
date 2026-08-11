#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

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
        }
    }

    fn open_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.folder = Some(path.clone());
            self.files = Self::collect_files(&path);
            self.current_index = 0;
            self.playing = false;
            self.player.stop();
            self.status = format!("Opened folder: {}", path.display());
        }
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
        self.current_index = (self.current_index + 1) % self.files.len();
        self.playing = false;
        self.player.stop();
    }

    fn previous_file(&mut self) {
        if self.files.is_empty() {
            return;
        }
        self.current_index = (self.current_index + self.files.len() - 1) % self.files.len();
        self.playing = false;
        self.player.stop();
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
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowRight)) {
            self.next_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::ArrowLeft)) {
            self.previous_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
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
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("file_grid")
                            .striped(true)
                            .min_col_width(60.0)
                            .show(ui, |ui| {
                                ui.strong("Name");
                                ui.strong("Size");
                                ui.end_row();
                                for (i, file) in self.files.iter().enumerate() {
                                    let selected = i == self.current_index;
                                    let name = file
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| file.display().to_string());
                                    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                                    if ui.selectable_label(selected, name).clicked() {
                                        self.current_index = i;
                                        self.playing = false;
                                        self.player.stop();
                                    }
                                    ui.label(format_size(size));
                                    ui.end_row();
                                }
                            });
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
            if let Some(delta) = sep_response.drag_delta() {
                self.left_width = (self.left_width + delta.x).clamp(120.0, 500.0);
            }
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
                    ui.label("Minlabel");
                    ui.label("A minimal image labeling tool.");
                    ui.label("Version 0.1.0");
                    ui.separator();
                    ui.hyperlink("https://github.com/Evi233/Minlabel_rust");
                });
        }
    }
}

impl MinlabelApp {
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
        let slider = egui::Slider::new(&mut pos, 0.0..=duration.max(0.001))
            .show_value(false)
            .trailing_fill(true);
        ui.spacing_mut().slider_width = ui.available_width();
        if ui.add(slider).changed() {
            self.player.seek(pos);
        }
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let play_btn = egui::Button::new(egui::RichText::new("\u{25B6}").size(18.0))
                .min_size(egui::vec2(36.0, 30.0));
            if ui.add(play_btn).clicked() {
                self.toggle_play();
            }

            let pause_btn = egui::Button::new(egui::RichText::new("\u{23F8}").size(18.0))
                .min_size(egui::vec2(36.0, 30.0));
            if ui.add(pause_btn).clicked() {
                self.player.pause();
                self.playing = false;
            }

            ui.menu_button("\u{1F3A7}", |ui| {
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
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        list.insert(0, "NotoSansSC".to_owned());
        list.push("NotoEmoji".to_owned());
    }
    ctx.set_fonts(fonts);
}
