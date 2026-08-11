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
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Right)) {
            self.next_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Left)) {
            self.previous_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
            self.toggle_play();
        }
    }
}

impl eframe::App for MinlabelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Folder").clicked() {
                        self.open_folder();
                        ui.close_menu();
                    }
                    if ui.button("Export").clicked() {
                        self.export();
                        ui.close_menu();
                    }
                    if ui.button("Connect").clicked() {
                        self.connect();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Next File").clicked() {
                        self.next_file();
                        ui.close_menu();
                    }
                    if ui.button("Previous File").clicked() {
                        self.previous_file();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Playback", |ui| {
                    if ui
                        .button(if self.playing { "Stop" } else { "Play" })
                        .clicked()
                    {
                        self.toggle_play();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
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
                .show(ctx, |ui| {
                    ui.label("Minlabel");
                    ui.label("A minimal image labeling tool.");
                    ui.label("Version 0.1.0");
                    ui.separator();
                    ui.hyperlink("https://github.com/Evi233/Minlabel_rust");
                });
        }
    }
}
