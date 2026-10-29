//! PR3's configuration (config/pr3.toml): which model each kind of device runs, the
//! pinned models it can choose from, and the limits on consequential actions. The
//! repository's file is built into the app; a `pr3.toml` in the app's data folder
//! replaces it at runtime.

use std::collections::BTreeMap;
use std::path::Path;

use pr3_core::trusted::{Contact, Directory, Limits};
use serde::Deserialize;

const BUILT_IN: &str = include_str!("../../../../config/pr3.toml");
pub const FILE_NAME: &str = "pr3.toml";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub model: ModelChoice,
    pub models: BTreeMap<String, ModelEntry>,
    #[serde(default)]
    pub traces: Traces,
    #[serde(default)]
    pub tasks: Tasks,
    #[serde(default)]
    pub actions: Actions,
    #[serde(default)]
    pub node: Node,
}

/// The private hybrid (docs Sec. 6.3): a computer lends its model to paired phones.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Node {
    /// On a computer: accept paired phones. Phones ignore it.
    pub serve: bool,
    /// Where to listen, `address:port`.
    pub listen: String,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            serve: true,
            listen: format!("127.0.0.1:{}", pr3_hybrid::DEFAULT_PORT),
        }
    }
}

/// Consequential actions (docs Sec. 4.6) and the user's limits on them (Sec. 9.1).
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Actions {
    /// The kill switch: false refuses every consequential action.
    pub enabled: bool,
    pub max_recipients: usize,
    pub max_per_day: usize,
    /// How long the confirmation card waits for the user's answer.
    pub approval_seconds: u64,
    /// The sandbox address book: the only people PR3 can email.
    pub contacts: Vec<Contact>,
}

impl Default for Actions {
    fn default() -> Self {
        let limits = Limits::default();
        Self {
            enabled: limits.enabled,
            max_recipients: limits.max_recipients,
            max_per_day: limits.max_per_day,
            approval_seconds: limits.approval_ms / 1000,
            contacts: Vec::new(),
        }
    }
}

impl Actions {
    pub fn limits(&self) -> Limits {
        Limits {
            enabled: self.enabled,
            max_recipients: self.max_recipients,
            max_per_day: self.max_per_day,
            approval_ms: self.approval_seconds.saturating_mul(1000),
            ..Limits::default()
        }
    }

    pub fn directory(&self) -> Directory {
        Directory {
            contacts: self.contacts.clone(),
        }
    }

    fn check(&self) -> Result<(), String> {
        for contact in &self.contacts {
            let address = &contact.address;
            let plausible = address.split('@').count() == 2
                && !address.starts_with('@')
                && !address.ends_with('@')
                && !address.chars().any(|c| c.is_whitespace() || c.is_control());
            if contact.name.trim().is_empty() || contact.name.contains(',') || !plausible {
                return Err(format!(
                    "[[actions.contacts]] '{}' <{address}> needs a name without commas and one address",
                    contact.name
                ));
            }
        }
        Ok(())
    }
}

/// OpenJarvis-format traces of every turn. Off unless the config turns them on: they
/// hold whole conversations in plain text.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct Traces {
    pub enabled: bool,
    /// The file is rotated at this size; one older file is kept.
    pub max_mb: u64,
}

impl Default for Traces {
    fn default() -> Self {
        Self {
            enabled: false,
            max_mb: 8,
        }
    }
}

/// How long the user's tasks are kept (docs Sec. 4.8).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tasks {
    /// Finished tasks are erased this many days after they last changed; unfinished
    /// ones stay until they finish. 0 keeps finished tasks until the app closes.
    pub keep_days: u64,
}

impl Default for Tasks {
    fn default() -> Self {
        Self { keep_days: 30 }
    }
}

/// Which model each kind of device runs, by id.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelChoice {
    pub desktop: String,
    /// For computers with less than `desktop_min_memory_gb` of memory.
    #[serde(default)]
    pub desktop_small: Option<String>,
    #[serde(default)]
    pub desktop_min_memory_gb: f64,
    pub mobile: String,
    /// For phones with less than `mobile_min_memory_gb` of memory.
    pub mobile_small: String,
    pub mobile_min_memory_gb: f64,
}

/// One pinned GGUF file and where it comes from: a Hugging Face repository at a fixed
/// revision. A download is kept only if it has exactly this size and SHA-256.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEntry {
    pub repo: String,
    pub revision: String,
    pub file: String,
    pub sha256: String,
    pub size: u64,
}

impl ModelEntry {
    /// Where the file is downloaded from.
    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.file
        )
    }

    /// A file name with no folders, a repository `owner/name` and a revision that stay
    /// inside the URL's path, and a SHA-256 in hex.
    fn check(&self, id: &str) -> Result<(), String> {
        let plain = |s: &str| {
            !s.is_empty()
                && s != "."
                && s != ".."
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        let repo = self.repo.split('/').collect::<Vec<_>>();
        let fine = plain(&self.file)
            && self.file.ends_with(".gguf")
            && repo.len() == 2
            && repo.iter().all(|part| plain(part))
            && plain(&self.revision)
            && self.sha256.len() == 64
            && self.sha256.chars().all(|c| c.is_ascii_hexdigit())
            && self.size > 0;
        if fine {
            Ok(())
        } else {
            Err(format!(
                "[models.\"{id}\"] needs a repo owner/name, a revision, a .gguf file name, \
                 a SHA-256 and a size"
            ))
        }
    }
}

/// The kind of device, with its physical memory in bytes if known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Device {
    Desktop {
        memory: Option<u64>,
    },
    /// A phone or tablet.
    Mobile {
        memory: Option<u64>,
    },
}

impl Device {
    pub fn current() -> Self {
        let memory = physical_memory();
        if cfg!(any(target_os = "ios", target_os = "android")) {
            Self::Mobile { memory }
        } else {
            Self::Desktop { memory }
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
        let named = [&choice.desktop, &choice.mobile, &choice.mobile_small]
            .into_iter()
            .chain(choice.desktop_small.as_ref());
        for id in named {
            if !config.models.contains_key(id) {
                return Err(format!(
                    "[model] names '{id}', which [models] does not list"
                ));
            }
        }
        for (id, entry) in &config.models {
            entry.check(id)?;
        }
        config.actions.check()?;
        Ok(config)
    }

    /// The model this device should run: phones with too little (or unknown) memory get
    /// the small one.
    pub fn model_for(&self, device: Device) -> Option<(&str, &ModelEntry)> {
        let choice = &self.model;
        let gb = |n: f64| n * 1024.0 * 1024.0 * 1024.0;
        let id = match device {
            // A computer whose memory is unknown gets the large model.
            Device::Desktop { memory } => match &choice.desktop_small {
                Some(small)
                    if memory.is_some_and(|m| (m as f64) < gb(choice.desktop_min_memory_gb)) =>
                {
                    small
                }
                _ => &choice.desktop,
            },
            // A phone whose memory is unknown gets the small one.
            Device::Mobile { memory } => {
                if memory.is_some_and(|m| m as f64 >= gb(choice.mobile_min_memory_gb)) {
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
        assert_eq!(config.tasks.keep_days, 30);
        assert_eq!(
            config
                .model_for(Device::Desktop { memory: None })
                .unwrap()
                .0,
            "qwen3.8-27b"
        );
        let desktop = |gb| Device::Desktop {
            memory: Some(gb * GB),
        };
        assert_eq!(config.model_for(desktop(96)).unwrap().0, "qwen3.8-27b");
        assert_eq!(config.model_for(desktop(16)).unwrap().0, "qwen3.5-4b");
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
            assert!(entry.url().starts_with("https://huggingface.co/"));
        }
    }

    #[test]
    fn a_model_must_stay_in_its_repository_and_carry_its_checksum() {
        let entry = &Config::built_in().unwrap().models["qwen3-0.6b"];
        assert_eq!(
            entry.url(),
            "https://huggingface.co/Qwen/Qwen3-0.6B-GGUF/resolve/\
             23749fefcc72300e3a2ad315e1317431b06b590a/Qwen3-0.6B-Q8_0.gguf"
        );
        for (from, to) in [
            (
                "file = \"Qwen3-0.6B-Q8_0.gguf\"",
                "file = \"../Qwen3-0.6B-Q8_0.gguf\"",
            ),
            (
                "file = \"Qwen3-0.6B-Q8_0.gguf\"",
                "file = \"Qwen3-0.6B-Q8_0.bin\"",
            ),
            ("repo = \"Qwen/Qwen3-0.6B-GGUF\"", "repo = \"Qwen/../evil\""),
            (
                "repo = \"Qwen/Qwen3-0.6B-GGUF\"",
                "repo = \"Qwen3-0.6B-GGUF\"",
            ),
            ("sha256 = \"9465e63a", "sha256 = \"zz65e63a"),
        ] {
            let text = BUILT_IN.replace(from, to);
            assert_ne!(text, BUILT_IN, "{from}");
            assert!(
                Config::parse(&text).unwrap_err().contains("qwen3-0.6b"),
                "{to}"
            );
        }
    }

    #[test]
    fn built_in_actions_have_limits_and_sandbox_contacts() {
        let actions = Config::built_in().unwrap().actions;
        assert!(actions.enabled);
        assert_eq!(actions.limits().approval_ms, 120_000);
        assert!(
            actions
                .contacts
                .iter()
                .all(|c| c.address.ends_with("@example.com"))
        );
        let bad = BUILT_IN.replace("alex@example.com", "alex example.com");
        assert!(Config::parse(&bad).unwrap_err().contains("Alex Rivera"));
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
            config
                .model_for(Device::Desktop { memory: None })
                .unwrap()
                .0,
            "qwen3.8-27b-q8"
        );

        std::fs::write(dir.join(FILE_NAME), "not toml [").unwrap();
        let config = Config::load(Some(&dir)).unwrap();
        assert_eq!(
            config
                .model_for(Device::Desktop { memory: None })
                .unwrap()
                .0,
            "qwen3.8-27b"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reads_this_macs_memory() {
        assert!(physical_memory().is_some_and(|m| m >= 4 * GB));
    }
}
