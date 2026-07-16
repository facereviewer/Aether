use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui;

use crate::preset::{Preset, PresetStore};

pub fn run() {
    let icon_bytes = include_bytes!("icon.png");
    let icon = eframe::icon_data::from_png_bytes(icon_bytes)
        .ok()
        .map(|d| std::sync::Arc::new(d));

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([820.0, 680.0])
        .with_min_inner_size([650.0, 500.0])
        .with_title("Aether");

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(icon_data);
    }

    let options = eframe::NativeOptions {
        viewport,
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
        connected_since: None,
        show_new_preset_dialog: false,
        new_preset_name: String::new(),
        show_delete_confirm: false,
        delete_target: String::new(),
    };

    eframe::run_native(
        "Aether",
        options,
        Box::new(|cc| {
            let mut style = (*cc.egui_ctx.style()).clone();
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            cc.egui_ctx.set_style(style);
            Ok(Box::new(app))
        }),
    )
    .ok();
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
    connected_since: Option<Instant>,
    show_new_preset_dialog: bool,
    new_preset_name: String,
    show_delete_confirm: bool,
    delete_target: String,
}

impl eframe::App for AetherGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.log_rx {
            if let Ok(logs) = rx.lock() {
                for msg in logs.iter() {
                    if !self.logs.contains(msg) {
                        self.logs.push(msg.clone());
                    }
                }
            }
        }

        self.render_top_bar(ctx);

        egui::SidePanel::left("presets")
            .resizable(true)
            .default_width(170.0)
            .min_width(140.0)
            .show(ctx, |ui| {
                self.render_presets_panel(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.render_config(ui);
            });
        });

        self.render_dialogs(ctx);
    }
}

impl AetherGui {
    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("Aether");
                ui.separator();

                let (status_text, status_color) = match &self.status {
                    Status::Idle => ("Ready".into(), egui::Color32::GRAY),
                    Status::Connecting => ("Connecting...".into(), egui::Color32::from_rgb(220, 180, 50)),
                    Status::Connected => {
                        let elapsed = self.connected_since.map(|t| t.elapsed().as_secs()).unwrap_or(0);
                        let mins = elapsed / 60;
                        let secs = elapsed % 60;
                        (format!("Connected ({:02}:{:02})", mins, secs), egui::Color32::from_rgb(60, 200, 60))
                    }
                    Status::Disconnecting => ("Disconnecting...".into(), egui::Color32::from_rgb(220, 150, 50)),
                    Status::Error(e) => (format!("Error: {}", e), egui::Color32::from_rgb(220, 60, 60)),
                };

                ui.label(egui::RichText::new(&status_text).color(status_color).strong());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (btn_text, btn_color) = match &self.status {
                        Status::Connected | Status::Connecting => ("Disconnect", egui::Color32::from_rgb(180, 50, 50)),
                        Status::Disconnecting => ("Disconnecting...", egui::Color32::from_rgb(150, 120, 50)),
                        _ => ("Connect", egui::Color32::from_rgb(50, 160, 50)),
                    };

                    if ui.add(
                        egui::Button::new(egui::RichText::new(btn_text).color(egui::Color32::WHITE).strong())
                            .fill(btn_color).min_size(egui::vec2(100.0, 28.0))
                    ).clicked() {
                        match &self.status {
                            Status::Idle | Status::Error(_) => self.connect(),
                            Status::Connected | Status::Connecting => self.disconnect(),
                            _ => {}
                        }
                    }
                });
            });
            ui.add_space(4.0);
        });
    }

    fn render_presets_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Presets");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+").on_hover_text("New preset").clicked() {
                    self.show_new_preset_dialog = true;
                    self.new_preset_name.clear();
                }
            });
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let names: Vec<String> = self.preset_store.presets.iter().map(|p| p.name.clone()).collect();
            for name in &names {
                let is_selected = self.selected_preset_name == *name;
                let response = ui.selectable_label(is_selected, name.as_str());
                if response.clicked() {
                    self.selected_preset_name = name.clone();
                    if let Some(p) = self.preset_store.presets.iter().find(|p| p.name == *name) {
                        self.editing = p.clone();
                    }
                }
                response.context_menu(|ui| {
                    if !self.is_builtin(name) && ui.button("Delete").clicked() {
                        self.show_delete_confirm = true;
                        self.delete_target = name.clone();
                        ui.close_menu();
                    }
                });
            }
        });
    }

    fn render_config(&mut self, ui: &mut egui::Ui) {
        // === Connection ===
        ui.group(|ui| {
            ui.strong("Connection");
            ui.horizontal(|ui| {
                ui.label("Protocol:");
                egui::ComboBox::from_id_salt("protocol").width(140.0)
                    .selected_text(&self.editing.protocol).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.editing.protocol, "masq".into(), "MASQUE (HTTP/3)");
                        ui.selectable_value(&mut self.editing.protocol, "wg".into(), "WireGuard");
                        ui.selectable_value(&mut self.editing.protocol, "gool".into(), "WARP-in-WARP");
                    });
                ui.separator();
                ui.label("Output:");
                egui::ComboBox::from_id_salt("output_mode").width(150.0)
                    .selected_text(if self.editing.tun_mode { "TUN (system-wide)" } else { "Mixed Proxy" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.editing.tun_mode, false, "Mixed Proxy (HTTP+SOCKS5)");
                        ui.selectable_value(&mut self.editing.tun_mode, true, "TUN (system-wide)");
                    });
            });

            if self.editing.tun_mode {
                ui.colored_label(egui::Color32::from_rgb(200, 180, 50), "TUN mode requires root privileges. Routes all system traffic through the tunnel.");
            } else {
                ui.horizontal(|ui| {
                    ui.label("Bind:");
                    ui.add(egui::TextEdit::singleline(&mut self.editing.bind).desired_width(180.0));
                });
            }
        });

        ui.add_space(4.0);

        // === Proxy Settings ===
        if !self.editing.tun_mode {
            ui.group(|ui| {
                ui.strong("Proxy Settings");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.editing.allow_lan, "Allow from LAN (bind 0.0.0.0)");
                    if self.editing.allow_lan {
                        self.editing.bind = self.editing.bind.replace("127.0.0.1", "0.0.0.0");
                    }
                });

                ui.checkbox(&mut self.editing.auth_enabled, "Enable authentication");
                if self.editing.auth_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Username:");
                        ui.add(egui::TextEdit::singleline(&mut self.editing.auth_user).desired_width(120.0).password(false));
                        ui.label("Password:");
                        ui.add(egui::TextEdit::singleline(&mut self.editing.auth_pass).desired_width(120.0).password(true));
                    });
                }

                ui.horizontal(|ui| {
                    let proxy_label = if self.editing.system_proxy { "System Proxy: ON" } else { "System Proxy: OFF" };
                    let proxy_color = if self.editing.system_proxy { egui::Color32::from_rgb(60, 180, 60) } else { egui::Color32::GRAY };

                    if ui.toggle_value(&mut self.editing.system_proxy, egui::RichText::new(proxy_label).color(proxy_color)).clicked() {
                        if self.editing.system_proxy {
                            if let Ok(addr) = self.editing.bind.parse::<std::net::SocketAddr>() {
                                let _ = crate::system_proxy::set_proxy(addr);
                            }
                        } else {
                            let _ = crate::system_proxy::clear_proxy();
                        }
                    }
                });
            });
        }

        ui.add_space(4.0);

        // === Scanning ===
        ui.group(|ui| {
            ui.strong("Endpoint Discovery");
            ui.horizontal(|ui| {
                ui.label("Scan:");
                egui::ComboBox::from_id_salt("scan").width(140.0)
                    .selected_text(&self.editing.scan_mode).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.editing.scan_mode, "turbo".into(), "Turbo (fast)");
                        ui.selectable_value(&mut self.editing.scan_mode, "balanced".into(), "Balanced");
                        ui.selectable_value(&mut self.editing.scan_mode, "thorough".into(), "Thorough (deep)");
                        ui.selectable_value(&mut self.editing.scan_mode, "stealth".into(), "Stealth (quiet)");
                    });
                ui.separator();
                ui.label("IP:");
                egui::ComboBox::from_id_salt("ip").width(80.0)
                    .selected_text(&self.editing.ip_version).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.editing.ip_version, "v4".into(), "IPv4");
                        ui.selectable_value(&mut self.editing.ip_version, "v6".into(), "IPv6");
                        ui.selectable_value(&mut self.editing.ip_version, "both".into(), "Both");
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Force peer:");
                ui.add(egui::TextEdit::singleline(&mut self.editing.peer).desired_width(200.0).hint_text("auto-detect if empty"));
            });
        });

        ui.add_space(4.0);

        // === Security ===
        ui.group(|ui| {
            ui.strong("Security & Obfuscation");
            ui.horizontal(|ui| {
                ui.label("MASQUE:");
                egui::ComboBox::from_id_salt("noize").width(110.0)
                    .selected_text(&self.editing.masque_obfuscation).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.editing.masque_obfuscation, "off".into(), "Off");
                        ui.selectable_value(&mut self.editing.masque_obfuscation, "firewall".into(), "Firewall");
                        ui.selectable_value(&mut self.editing.masque_obfuscation, "gfw".into(), "GFW");
                    });
                ui.separator();
                ui.label("WireGuard:");
                egui::ComboBox::from_id_salt("aethernoize").width(110.0)
                    .selected_text(&self.editing.wg_obfuscation).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.editing.wg_obfuscation, "off".into(), "Off");
                        ui.selectable_value(&mut self.editing.wg_obfuscation, "light".into(), "Light");
                        ui.selectable_value(&mut self.editing.wg_obfuscation, "balanced".into(), "Balanced");
                        ui.selectable_value(&mut self.editing.wg_obfuscation, "aggressive".into(), "Aggressive");
                    });
            });
            ui.horizontal(|ui| {
                ui.label("ECH:");
                egui::ComboBox::from_id_salt("ech").width(100.0)
                    .selected_text(&self.editing.ech).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.editing.ech, "off".into(), "Off");
                        ui.selectable_value(&mut self.editing.ech, "auto".into(), "Auto");
                    });
                ui.separator();
                ui.label("Keepalive:");
                ui.add(egui::DragValue::new(&mut self.editing.wg_keepalive).speed(1).range(0..=120));
                ui.label("s");
            });
        });

        ui.add_space(4.0);

        // === Advanced ===
        ui.collapsing("Advanced", |ui| {
            ui.horizontal(|ui| {
                ui.label("Config file:");
                ui.add(egui::TextEdit::singleline(&mut self.editing.config_path).desired_width(250.0));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.editing.wg_no_profile_retry, "No WG profile retry");
                ui.separator();
                ui.checkbox(&mut self.editing.verbose, "Verbose logging");
            });
        });

        ui.add_space(6.0);

        // === Log ===
        ui.separator();
        ui.strong("Log");
        egui::ScrollArea::vertical().max_height(130.0).stick_to_bottom(true).show(ui, |ui| {
            if self.logs.is_empty() {
                ui.label(egui::RichText::new("No activity yet.").italics().weak());
            } else {
                for line in &self.logs {
                    ui.label(egui::RichText::new(line).monospace().small());
                }
            }
        });
    }

    fn render_dialogs(&mut self, ctx: &egui::Context) {
        if self.show_new_preset_dialog {
            egui::Window::new("New Preset").collapsible(false).resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.new_preset_name);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() && !self.new_preset_name.is_empty() {
                            let mut p = self.editing.clone();
                            p.name = self.new_preset_name.clone();
                            self.preset_store.presets.push(p);
                            self.selected_preset_name = self.new_preset_name.clone();
                            let _ = self.preset_store.save();
                            self.show_new_preset_dialog = false;
                        }
                        if ui.button("Cancel").clicked() { self.show_new_preset_dialog = false; }
                    });
                });
        }

        if self.show_delete_confirm {
            egui::Window::new("Delete Preset").collapsible(false).resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    ui.label(format!("Delete '{}'?", self.delete_target));
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            self.preset_store.delete(&self.delete_target);
                            self.selected_preset_name = self.preset_store.active_preset().name.clone();
                            self.editing = self.preset_store.active_preset().clone();
                            let _ = self.preset_store.save();
                            self.show_delete_confirm = false;
                        }
                        if ui.button("Cancel").clicked() { self.show_delete_confirm = false; }
                    });
                });
        }
    }

    fn is_builtin(&self, name: &str) -> bool {
        PresetStore::built_in().iter().any(|p| p.name == name)
    }

    fn connect(&mut self) {
        self.status = Status::Connecting;
        self.connected_since = Some(Instant::now());
        self.logs.clear();
        self.logs.push("[GUI] Starting...".to_string());

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
        self.connected_since = None;
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
    if preset.tun_mode {
        push_log(&logs, "[GUI] Mode: TUN");
    } else {
        let proxy_type = "HTTP+SOCKS5";
        let auth_info = if preset.auth_enabled { " (auth enabled)" } else { "" };
        let lan_info = if preset.allow_lan { " (LAN)" } else { "" };
        push_log(&logs, &format!("[GUI] Mode: {} on {}{}{}", proxy_type, preset.bind, auth_info, lan_info));
    }

    let listen: std::net::SocketAddr = match preset.bind.parse() {
        Ok(addr) => addr,
        Err(e) => { push_log(&logs, &format!("[GUI] Bad bind: {e}")); return; }
    };

    // Set env vars from preset (upstream uses env vars)
    std::env::set_var("AETHER_SOCKS", &preset.bind);
    std::env::set_var("AETHER_PROTOCOL", &preset.protocol);
    std::env::set_var("AETHER_SCAN", &preset.scan_mode);
    std::env::set_var("AETHER_IP", &preset.ip_version);
    std::env::set_var("AETHER_NOIZE", &preset.masque_obfuscation);
    std::env::set_var("AETHER_CONFIG", &preset.config_path);
    if !preset.peer.is_empty() {
        std::env::set_var("AETHER_PEER", &preset.peer);
    }
    if preset.ech != "off" {
        std::env::set_var("AETHER_ECH", &preset.ech);
    }
    std::env::set_var("AETHER_WG_KEEPALIVE", &preset.wg_keepalive.to_string());
    if preset.wg_no_profile_retry {
        std::env::set_var("AETHER_WG_NO_PROFILE_RETRY", "1");
    }
    if preset.verbose {
        std::env::set_var("RUST_LOG", "debug");
    }

    let protocol = crate::Protocol::parse(&preset.protocol);
    push_log(&logs, &format!("[GUI] Protocol: {}", protocol.label()));

    let result: Result<()> = match protocol {
        crate::Protocol::Masque => {
            let config_path = crate::masque_config_path(&preset.config_path);
            match crate::load_or_provision_masque(&config_path).await {
                Ok(identity) => {
                    push_log(&logs, &format!("[GUI] Identity: {}", identity.device_id));
                    match crate::select_peer(&identity, protocol).await {
                        Ok(peer) => {
                            push_log(&logs, &format!("[GUI] Peer: {peer}"));
                            let ech = crate::resolve_ech().await;
                            crate::run_masque_tunnel(&identity, peer, ech, listen).await
                        }
                        Err(e) => { push_log(&logs, &format!("[GUI] Peer error: {e}")); Err(e) }
                    }
                }
                Err(e) => { push_log(&logs, &format!("[GUI] Identity error: {e}")); Err(e) }
            }
        }
        crate::Protocol::WireGuard => {
            let config_path = crate::warp_config_path(&preset.config_path);
            let lastconn = crate::lastconn_path(&config_path);
            match crate::load_or_provision_warp(&config_path).await {
                Ok(identity) => {
                    push_log(&logs, &format!("[GUI] Identity: {}", identity.device_id));
                    crate::run_wireguard(identity, listen, lastconn).await
                }
                Err(e) => { push_log(&logs, &format!("[GUI] Identity error: {e}")); Err(e) }
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
                    match crate::select_peer(&primary, crate::Protocol::WireGuard).await {
                        Ok(peer) => {
                            push_log(&logs, &format!("[GUI] Peer: {peer}"));
                            crate::run_warp_in_warp(primary, secondary, peer, listen).await
                        }
                        Err(e) => { push_log(&logs, &format!("[GUI] Peer error: {e}")); Err(e) }
                    }
                }
                (Err(e), _) | (_, Err(e)) => { push_log(&logs, &format!("[GUI] Identity error: {e}")); Err(e) }
            }
        }
    };

    match result {
        Ok(()) => push_log(&logs, "[GUI] Tunnel closed."),
        Err(e) => push_log(&logs, &format!("[GUI] Error: {e}")),
    }
}
