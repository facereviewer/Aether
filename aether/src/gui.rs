use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::preset::{Preset, PresetStore};

pub fn run(_cli: crate::cli::Cli) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([700.0, 550.0])
            .with_min_inner_size([500.0, 400.0])
            .with_title(format!("Aether v{}", env!("CARGO_PKG_VERSION"))),
        ..Default::default()
    };

    let preset_store = PresetStore::load();
    let initial = preset_store.active_preset().clone();

    let app = AetherGui {
        preset_store,
        selected_preset_name: initial.name.clone(),
        editing: initial,
        status: Status::Idle,
        logs: Vec::new(),
        log_rx: None,
        shutdown_tx: None,
        show_new_preset_dialog: false,
        new_preset_name: String::new(),
        show_delete_confirm: false,
        delete_target: String::new(),
    };

    eframe::run_native(
        &format!("Aether v{}", env!("CARGO_PKG_VERSION")),
        options,
        Box::new(|cc| {
            setup_custom_fonts(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
    .ok();
}

fn setup_custom_fonts(_ctx: &egui::Context) {
    // Use default fonts for now
}

#[derive(Debug, Clone, PartialEq)]
enum Status {
    Idle,
    Connecting,
    Connected,
    Disconnecting,
    Error(String),
}

struct AetherGui {
    preset_store: PresetStore,
    selected_preset_name: String,
    editing: Preset,
    status: Status,
    logs: Vec<String>,
    log_rx: Option<Arc<Mutex<Vec<String>>>>,
    shutdown_tx: Option<std::sync::mpsc::Sender<()>>,
    show_new_preset_dialog: bool,
    new_preset_name: String,
    show_delete_confirm: bool,
    delete_target: String,
}

impl eframe::App for AetherGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll log messages from background thread
        if let Some(rx) = &self.log_rx {
            if let Ok(logs) = rx.lock() {
                for msg in logs.iter() {
                    if !self.logs.contains(msg) {
                        self.logs.push(msg.clone());
                    }
                }
            }
        }

        // Left panel: presets
        egui::SidePanel::left("presets_panel").show(ctx, |ui| {
            ui.heading("Presets");
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                let preset_names: Vec<String> =
                    self.preset_store.presets.iter().map(|p| p.name.clone()).collect();
                for name in &preset_names {
                    let is_selected = self.selected_preset_name == *name;
                    let is_active = self.preset_store.active.as_deref() == Some(name.as_str());

                    let label = if is_active {
                        format!("{} *", name)
                    } else {
                        name.clone()
                    };

                    if ui.selectable_label(is_selected, &label).clicked() {
                        self.selected_preset_name = name.clone();
                        if let Some(preset) = self.preset_store.presets.iter().find(|p| p.name == *name) {
                            self.editing = preset.clone();
                        }
                    }
                }
            });

            ui.add_space(8.0);
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("+ New").clicked() {
                    self.show_new_preset_dialog = true;
                    self.new_preset_name.clear();
                }
                if ui.button("Save").clicked() {
                    if let Some(p) = self.preset_store.presets.iter_mut().find(|p| p.name == self.selected_preset_name) {
                        *p = self.editing.clone();
                        let _ = self.preset_store.save();
                    }
                }
                if ui.button("Delete").clicked() {
                    if !self.is_builtin(&self.selected_preset_name) {
                        self.show_delete_confirm = true;
                        self.delete_target = self.selected_preset_name.clone();
                    }
                }
            });
        });

        // Central panel: config + status + logs
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Configuration");
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Protocol
                ui.horizontal(|ui| {
                    ui.label("Protocol:");
                    egui::ComboBox::from_id_salt("protocol")
                        .selected_text(&self.editing.protocol)
                        .show_ui(ui, |ui| {
                            for opt in &["masq", "wg", "gool"] {
                                ui.selectable_value(&mut self.editing.protocol, opt.to_string(), *opt);
                            }
                        });
                });

                // Bind
                ui.horizontal(|ui| {
                    ui.label("Bind:");
                    ui.text_edit_singleline(&mut self.editing.bind);
                });

                // Scan mode
                ui.horizontal(|ui| {
                    ui.label("Scan:");
                    egui::ComboBox::from_id_salt("scan")
                        .selected_text(&self.editing.scan_mode)
                        .show_ui(ui, |ui| {
                            for opt in &["turbo", "balanced", "thorough", "stealth"] {
                                ui.selectable_value(&mut self.editing.scan_mode, opt.to_string(), *opt);
                            }
                        });
                });

                // IP version
                ui.horizontal(|ui| {
                    ui.label("IP version:");
                    egui::ComboBox::from_id_salt("ip")
                        .selected_text(&self.editing.ip_version)
                        .show_ui(ui, |ui| {
                            for opt in &["v4", "v6", "both"] {
                                ui.selectable_value(&mut self.editing.ip_version, opt.to_string(), *opt);
                            }
                        });
                });

                ui.separator();

                // MASQUE obfuscation
                ui.horizontal(|ui| {
                    ui.label("MASQUE Obsc:");
                    egui::ComboBox::from_id_salt("noize")
                        .selected_text(&self.editing.masque_obfuscation)
                        .show_ui(ui, |ui| {
                            for opt in &["off", "gfw", "firewall"] {
                                ui.selectable_value(&mut self.editing.masque_obfuscation, opt.to_string(), *opt);
                            }
                        });
                });

                // WG obfuscation
                ui.horizontal(|ui| {
                    ui.label("WG Obsc:");
                    egui::ComboBox::from_id_salt("aethernoize")
                        .selected_text(&self.editing.wg_obfuscation)
                        .show_ui(ui, |ui| {
                            for opt in &["off", "light", "balanced", "aggressive"] {
                                ui.selectable_value(&mut self.editing.wg_obfuscation, opt.to_string(), *opt);
                            }
                        });
                });

                // ECH
                ui.horizontal(|ui| {
                    ui.label("ECH:");
                    egui::ComboBox::from_id_salt("ech")
                        .selected_text(&self.editing.ech)
                        .show_ui(ui, |ui| {
                            for opt in &["off", "auto"] {
                                ui.selectable_value(&mut self.editing.ech, opt.to_string(), *opt);
                            }
                        });
                });

                // Peer
                ui.horizontal(|ui| {
                    ui.label("Peer:");
                    ui.text_edit_singleline(&mut self.editing.peer);
                    if ui.small_button("Clear").clicked() {
                        self.editing.peer.clear();
                    }
                });

                ui.separator();

                // WG keepalive
                ui.horizontal(|ui| {
                    ui.label("WG Keepalive:");
                    ui.add(egui::DragValue::new(&mut self.editing.wg_keepalive).speed(1).range(0..=120));
                    ui.label("s");
                });

                // Config path
                ui.horizontal(|ui| {
                    ui.label("Config:");
                    ui.text_edit_singleline(&mut self.editing.config_path);
                });

                // Checkboxes
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.editing.wg_no_profile_retry, "No profile retry");
                    ui.checkbox(&mut self.editing.verbose, "Verbose");
                });
            });

            ui.separator();

            // Status + Connect/Disconnect
            ui.horizontal(|ui| {
                let status_text = match &self.status {
                    Status::Idle => "Idle".to_string(),
                    Status::Connecting => "Connecting...".to_string(),
                    Status::Connected => "Connected".to_string(),
                    Status::Disconnecting => "Disconnecting...".to_string(),
                    Status::Error(e) => format!("Error: {}", e),
                };
                let status_color = match &self.status {
                    Status::Connected => egui::Color32::from_rgb(80, 200, 80),
                    Status::Error(_) => egui::Color32::from_rgb(200, 80, 80),
                    Status::Idle => egui::Color32::GRAY,
                    _ => egui::Color32::from_rgb(200, 180, 80),
                };

                match &self.status {
                    Status::Idle | Status::Error(_) => {
                        if ui.button("Connect").clicked() {
                            self.connect();
                        }
                    }
                    Status::Connected | Status::Connecting => {
                        if ui.button("Disconnect").clicked() {
                            self.disconnect();
                        }
                    }
                    Status::Disconnecting => {
                        ui.button("Disconnecting...").clicked();
                    }
                }

                ui.label(egui::RichText::new(status_text).color(status_color));
            });

            ui.separator();

            // Log area
            ui.label("Log:");
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.logs {
                        ui.label(egui::RichText::new(line).monospace().small());
                    }
                });
        });

        // New preset dialog
        if self.show_new_preset_dialog {
            egui::Window::new("New Preset")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.new_preset_name);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            if !self.new_preset_name.is_empty() {
                                let mut preset = self.editing.clone();
                                preset.name = self.new_preset_name.clone();
                                self.preset_store.presets.push(preset);
                                self.selected_preset_name = self.new_preset_name.clone();
                                let _ = self.preset_store.save();
                                self.show_new_preset_dialog = false;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_new_preset_dialog = false;
                        }
                    });
                });
        }

        // Delete confirmation dialog
        if self.show_delete_confirm {
            egui::Window::new("Delete Preset")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!("Delete preset '{}'?", self.delete_target));
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            self.preset_store.delete(&self.delete_target);
                            self.selected_preset_name = self.preset_store.active_preset().name.clone();
                            self.editing = self.preset_store.active_preset().clone();
                            let _ = self.preset_store.save();
                            self.show_delete_confirm = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_delete_confirm = false;
                        }
                    });
                });
        }
    }
}

impl AetherGui {
    fn is_builtin(&self, name: &str) -> bool {
        PresetStore::built_in().iter().any(|p| p.name == name)
    }

    fn connect(&mut self) {
        self.status = Status::Connecting;
        self.logs.clear();
        self.logs.push("[GUI] Starting tunnel...".to_string());

        let preset = self.editing.clone();
        let logs_arc: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        self.log_rx = Some(logs_arc.clone());

        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
        self.shutdown_tx = Some(shutdown_tx);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(async move {
                run_tunnel_from_preset(preset, logs_arc, shutdown_rx).await;
            });
        });
    }

    fn disconnect(&mut self) {
        self.status = Status::Disconnecting;
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.status = Status::Idle;
        self.logs.push("[GUI] Disconnected.".to_string());
    }
}

async fn run_tunnel_from_preset(
    preset: Preset,
    logs: Arc<Mutex<Vec<String>>>,
    _shutdown_rx: std::sync::mpsc::Receiver<()>,
) {
    use crate::error::Result;

    fn push_log(logs: &Arc<Mutex<Vec<String>>>, msg: &str) {
        if let Ok(mut buffer) = logs.lock() {
            buffer.push(msg.to_string());
        }
    }

    push_log(&logs, &format!("[GUI] Protocol: {}", preset.protocol));
    push_log(&logs, &format!("[GUI] Scan: {}", preset.scan_mode));
    push_log(&logs, &format!("[GUI] Binding to {}", preset.bind));

    let listen: std::net::SocketAddr = match preset.bind.parse() {
        Ok(addr) => addr,
        Err(e) => {
            push_log(&logs, &format!("[GUI] Bad bind address: {e}"));
            return;
        }
    };

    // Build a Cli from the preset
    let cli_args = crate::cli::Cli {
        bind: Some(preset.bind.clone()),
        mode: Some(preset.protocol.clone()),
        scan: Some(preset.scan_mode.clone()),
        config: Some(preset.config_path.clone()),
        ip: Some(preset.ip_version.clone()),
        noize: Some(preset.masque_obfuscation.clone()),
        aethernoize: Some(preset.wg_obfuscation.clone()),
        peer: if preset.peer.is_empty() { None } else { Some(preset.peer.clone()) },
        ech: Some(preset.ech.clone()),
        wg_keepalive: Some(preset.wg_keepalive),
        wg_no_profile_retry: preset.wg_no_profile_retry,
        verbose: preset.verbose,
        gui: false,
    };

    let protocol = crate::Protocol::parse(&preset.protocol);
    push_log(&logs, &format!("[GUI] Resolved protocol: {}", protocol.label()));

    // Run the tunnel (this blocks until shutdown or error)
    let result: Result<()> = match protocol {
        crate::Protocol::Masque => {
            let config_path = crate::masque_config_path(&preset.config_path);
            match crate::load_or_provision_masque(&config_path).await {
                Ok(identity) => {
                    push_log(&logs, &format!("[GUI] Identity ready: {}", identity.device_id));
                    match crate::select_peer(&identity, protocol, &cli_args).await {
                        Ok(peer) => {
                            push_log(&logs, &format!("[GUI] Peer: {peer}"));
                            let ech = crate::resolve_ech(&cli_args).await;
                            crate::run_masque_tunnel(identity, peer, ech, listen, &cli_args).await
                        }
                        Err(e) => {
                            push_log(&logs, &format!("[GUI] Peer selection failed: {e}"));
                            Err(e)
                        }
                    }
                }
                Err(e) => {
                    push_log(&logs, &format!("[GUI] Identity failed: {e}"));
                    Err(e)
                }
            }
        }
        crate::Protocol::WireGuard => {
            let config_path = crate::warp_config_path(&preset.config_path);
            match crate::load_or_provision_warp(&config_path).await {
                Ok(identity) => {
                    push_log(&logs, &format!("[GUI] Identity ready: {}", identity.device_id));
                    crate::run_wireguard(identity, listen, &cli_args).await
                }
                Err(e) => {
                    push_log(&logs, &format!("[GUI] Identity failed: {e}"));
                    Err(e)
                }
            }
        }
        crate::Protocol::WarpInWarp => {
            let primary_path = crate::warp_config_path(&preset.config_path);
            let secondary_path = crate::derive_sibling_path(&primary_path, "secondary");
            match (
                crate::load_or_provision_warp(&primary_path).await,
                crate::load_or_provision_warp(&secondary_path).await,
            ) {
                (Ok(primary), Ok(secondary)) => {
                    push_log(&logs, &format!("[GUI] Outer: {} Inner: {}", primary.device_id, secondary.device_id));
                    match crate::select_peer(&primary, crate::Protocol::WireGuard, &cli_args).await {
                        Ok(peer) => {
                            push_log(&logs, &format!("[GUI] Peer: {peer}"));
                            crate::run_warp_in_warp(primary, secondary, peer, listen, &cli_args).await
                        }
                        Err(e) => {
                            push_log(&logs, &format!("[GUI] Peer selection failed: {e}"));
                            Err(e)
                        }
                    }
                }
                (Err(e), _) | (_, Err(e)) => {
                    push_log(&logs, &format!("[GUI] Identity failed: {e}"));
                    Err(e)
                }
            }
        }
    };

    match result {
        Ok(()) => push_log(&logs, "[GUI] Tunnel closed."),
        Err(e) => push_log(&logs, &format!("[GUI] Tunnel error: {e}")),
    }
}
