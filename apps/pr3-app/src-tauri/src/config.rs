//! PR3's configuration (config/pr3.toml): which model each kind of device runs, and the
//! pinned models it can choose from. The repository's file is built into the app; a
//! `pr3.toml` in the app's data folder replaces it at runtime.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

const BUILT_IN: &str = include_str!("../../../../config/pr3.toml");
pub const FILE_NAME: &str = "pr3.toml";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub model: ModelChoice,
    pub models: BTreeMap<String, ModelEntry>,
}

/// Which model each kind of device runs, by id.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelChoice {
    pub desktop: String,
    pub mobile: String,
    /// For phones with less than `mobile_min_memory_gb` of memory.
    pub mobile_small: String,
    pub mobile_min_memory_gb: f64,
}

/// One pinned GGUF file. The app needs only its name; the repository, revision and
/// SHA-256 are for scripts/fetch-model.sh.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    pub file: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Device {
    Desktop,
    /// A phone or tablet, with its physical memory in bytes if known.
    Mobile {
        memory: Option<u64>,
    },
}

impl Device {
    pub fn current() -> Self {
        if cfg!(any(target_os = "ios", target_os = "android")) {
            Self::Mobile {
                memory: physical_memory(),
            }
        } else {
            Self::Desktop
        }
    }
}

impl Config {
    pub fn built_in() -> Result<Self, String> {
        Self::parse(BUILT_IN)
    }

    /// The data folder's `pr3.toml` if there is one, otherwise the built-in config. A
    /// broken override is reported and ignored, so the app still starts.
    pub fn load(data_dir: Option<&Path>) -> Result<Self, String> {
        let path = data_dir.map(|dir| dir.join(FILE_NAME));
        if let Some(path) = path.filter(|p| p.is_file()) {
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| Self::parse(&text))
            {
                Ok(config) => return Ok(config),
                Err(e) => eprintln!("PR3: ignoring {}: {e}", path.display()),
            }
        }
        Self::built_in()
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let config: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        let choice = &config.model;
        for id in [&choice.desktop, &choice.mobile, &choice.mobile_small] {
            if !config.models.contains_key(id) {
                return Err(format!(
                    "[model] names '{id}', which [models] does not list"
                ));
            }
        }
        Ok(config)
    }

    /// The model this device should run: phones with too little (or unknown) memory get
    /// the small one.
    pub fn model_for(&self, device: Device) -> Option<(&str, &ModelEntry)> {
        let choice = &self.model;
        let id = match device {
            Device::Desktop => &choice.desktop,
            Device::Mobile { memory } => {
                let min = choice.mobile_min_memory_gb * 1024.0 * 1024.0 * 1024.0;
                if memory.is_some_and(|m| m as f64 >= min) {
                    &choice.mobile
                } else {
                    &choice.mobile_small
                }
            }
        };
        self.models.get(id).map(|entry| (id.as_str(), entry))
    }
}

/// Physical memory in bytes, from the kernel.
#[cfg(target_os = "android")]
fn physical_memory() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = info.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// Physical memory in bytes, from the kernel. The iOS simulator reports the Mac's.
#[cfg(target_vendor = "apple")]
fn physical_memory() -> Option<u64> {
    let mut bytes: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: the name is a NUL-terminated C string, and `bytes`/`len` describe a valid,
    // writable u64 that hw.memsize fills; no new value is set (null, 0).
    let status = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&raw mut bytes).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0 && len == std::mem::size_of::<u64>()).then_some(bytes)
}

#[cfg(not(any(target_os = "android", target_vendor = "apple")))]
fn physical_memory() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn built_in_config_is_valid_and_picks_per_device() {
        let config = Config::built_in().unwrap();
        assert_eq!(config.model_for(Device::Desktop).unwrap().0, "qwen3.8-27b");
        let phone = |gb| Device::Mobile {
            memory: Some(gb * GB),
        };
        assert_eq!(config.model_for(phone(8)).unwrap().0, "qwen3.5-4b");
        assert_eq!(config.model_for(phone(6)).unwrap().0, "qwen3.5-2b");
        assert_eq!(
            config.model_for(Device::Mobile { memory: None }).unwrap().0,
            "qwen3.5-2b"
        );
        for entry in config.models.values() {
            assert!(entry.file.ends_with(".gguf"));
        }
    }

    #[test]
    fn unknown_model_id_is_rejected() {
        let text = BUILT_IN.replace("desktop = \"qwen3.8-27b\"", "desktop = \"qwen9\"");
        assert!(Config::parse(&text).unwrap_err().contains("qwen9"));
    }

    #[test]
    fn data_folder_override_wins_and_broken_one_is_ignored() {
        let dir = std::env::temp_dir().join(format!("pr3-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = BUILT_IN.replace("desktop = \"qwen3.8-27b\"", "desktop = \"qwen3.8-27b-q8\"");
        std::fs::write(dir.join(FILE_NAME), text).unwrap();
        let config = Config::load(Some(&dir)).unwrap();
        assert_eq!(
            config.model_for(Device::Desktop).unwrap().0,
            "qwen3.8-27b-q8"
        );

        std::fs::write(dir.join(FILE_NAME), "not toml [").unwrap();
        let config = Config::load(Some(&dir)).unwrap();
        assert_eq!(config.model_for(Device::Desktop).unwrap().0, "qwen3.8-27b");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reads_this_macs_memory() {
        assert!(physical_memory().is_some_and(|m| m >= 4 * GB));
    }
}
