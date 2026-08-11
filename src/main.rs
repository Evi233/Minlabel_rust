#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use eframe::egui;

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
}

impl MinlabelApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            folder: None,
            files: Vec::new(),
            current_index: 0,
            playing: false,
            show_about: false,
            status: String::new(),
        }
    }

    fn open_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.folder = Some(path.clone());
            self.files = Self::collect_files(&path);
            self.current_index = 0;
            self.playing = false;
            self.status = format!("Opened folder: {}", path.display());
        }
    }

    fn collect_files(dir: &PathBuf) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
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
    }

    fn previous_file(&mut self) {
        if self.files.is_empty() {
            return;
        }
        self.current_index = (self.current_index + self.files.len() - 1) % self.files.len();
        self.playing = false;
    }

    fn toggle_play(&mut self) {
        if self.files.is_empty() {
            self.status = "No files loaded. Open a folder first.".to_string();
            return;
        }
        self.playing = !self.playing;
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
            .resizable(true)
            .default_size(220.0)
            .min_size(120.0)
            .show(ui, |ui| {
                ui.heading("Files");
                ui.separator();
                if self.files.is_empty() {
                    ui.label("No folder opened.");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (i, file) in self.files.iter().enumerate() {
                            let selected = i == self.current_index;
                            let name = file
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| file.display().to_string());
                            if ui.selectable_label(selected, name).clicked() {
                                self.current_index = i;
                                self.playing = false;
                            }
                        }
                    });
                }
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Minlabel");
            ui.separator();
            match &self.folder {
                Some(folder) => {
                    ui.label(format!("Folder: {}", folder.display()));
                }
                None => {
                    ui.label("No folder opened. Use File > Open Folder.");
                }
            }
            ui.label(format!(
                "Files: {}  |  Current: {} / {}",
                self.files.len(),
                if self.files.is_empty() {
                    0
                } else {
                    self.current_index + 1
                },
                self.files.len()
            ));
            if let Some(file) = self.current_file() {
                ui.label(format!("Current file: {}", file.display()));
            }
            ui.label(format!(
                "Playback: {}",
                if self.playing { "Playing" } else { "Stopped" }
            ));
            if !self.status.is_empty() {
                ui.separator();
                ui.label(&self.status);
            }
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
