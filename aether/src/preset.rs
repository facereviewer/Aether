use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub protocol: String,
    pub bind: String,
    pub scan_mode: String,
    pub ip_version: String,
    pub masque_obfuscation: String,
    pub wg_obfuscation: String,
    pub ech: String,
    pub peer: String,
    pub config_path: String,
    pub wg_keepalive: u16,
    pub wg_no_profile_retry: bool,
    pub verbose: bool,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            protocol: "masq".to_string(),
            bind: "127.0.0.1:1819".to_string(),
            scan_mode: "balanced".to_string(),
            ip_version: "v4".to_string(),
            masque_obfuscation: "firewall".to_string(),
            wg_obfuscation: "balanced".to_string(),
            ech: "off".to_string(),
            peer: String::new(),
            config_path: "aether.toml".to_string(),
            wg_keepalive: 5,
            wg_no_profile_retry: false,
            verbose: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetStore {
    pub presets: Vec<Preset>,
    pub active: Option<String>,
}

impl Default for PresetStore {
    fn default() -> Self {
        Self {
            presets: Self::built_in(),
            active: Some("Default MASQUE".to_string()),
        }
    }
}

impl PresetStore {
    pub fn built_in() -> Vec<Preset> {
        vec![
            Preset {
                name: "Default MASQUE".to_string(),
                protocol: "masq".to_string(),
                bind: "127.0.0.1:1819".to_string(),
                scan_mode: "balanced".to_string(),
                ip_version: "v4".to_string(),
                masque_obfuscation: "firewall".to_string(),
                wg_obfuscation: "balanced".to_string(),
                ech: "off".to_string(),
                peer: String::new(),
                config_path: "aether.toml".to_string(),
                wg_keepalive: 5,
                wg_no_profile_retry: false,
                verbose: false,
            },
            Preset {
                name: "Fast WireGuard".to_string(),
                protocol: "wg".to_string(),
                bind: "127.0.0.1:1819".to_string(),
                scan_mode: "turbo".to_string(),
                ip_version: "v4".to_string(),
                masque_obfuscation: "firewall".to_string(),
                wg_obfuscation: "light".to_string(),
                ech: "off".to_string(),
                peer: String::new(),
                config_path: "aether.toml".to_string(),
                wg_keepalive: 5,
                wg_no_profile_retry: false,
                verbose: false,
            },
            Preset {
                name: "Stealth MASQUE".to_string(),
                protocol: "masq".to_string(),
                bind: "127.0.0.1:1819".to_string(),
                scan_mode: "stealth".to_string(),
                ip_version: "both".to_string(),
                masque_obfuscation: "gfw".to_string(),
                wg_obfuscation: "balanced".to_string(),
                ech: "auto".to_string(),
                peer: String::new(),
                config_path: "aether.toml".to_string(),
                wg_keepalive: 5,
                wg_no_profile_retry: false,
                verbose: false,
            },
            Preset {
                name: "WARP-in-WARP".to_string(),
                protocol: "gool".to_string(),
                bind: "127.0.0.1:1819".to_string(),
                scan_mode: "balanced".to_string(),
                ip_version: "v4".to_string(),
                masque_obfuscation: "firewall".to_string(),
                wg_obfuscation: "balanced".to_string(),
                ech: "off".to_string(),
                peer: String::new(),
                config_path: "aether.toml".to_string(),
                wg_keepalive: 5,
                wg_no_profile_retry: false,
                verbose: false,
            },
        ]
    }

    pub fn config_path() -> PathBuf {
        let config_dir = dirs().unwrap_or_else(|| PathBuf::from("."));
        config_dir.join("presets.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(store) = toml::from_str::<PresetStore>(&text) {
                    return store;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn active_preset(&self) -> &Preset {
        self.active
            .as_ref()
            .and_then(|name| self.presets.iter().find(|p| &p.name == name))
            .unwrap_or(&self.presets[0])
    }

    pub fn delete(&mut self, name: &str) -> bool {
        if self.presets.iter().any(|p| p.name == name) {
            self.presets.retain(|p| p.name != name);
            if self.active.as_deref() == Some(name) {
                self.active = self.presets.first().map(|p| p.name.clone());
            }
            true
        } else {
            false
        }
    }
}

fn dirs() -> Option<PathBuf> {
    // Try XDG_CONFIG_HOME first, then home dir
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("aether"));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".config").join("aether"));
    }
    // Windows
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Some(PathBuf::from(appdata).join("aether"));
    }
    Some(PathBuf::from("aether"))
}
