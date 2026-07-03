use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WatchRule {
    pub id: usize,
    pub path: String,
    pub target: String,
    #[serde(default)]
    pub listen_media: bool,
    #[serde(default)]
    pub auto_delete: Option<bool>,
}

impl WatchRule {
    pub fn get_auto_delete(&self, global_auto_delete: Option<bool>) -> bool {
        self.auto_delete.or(global_auto_delete).unwrap_or(false)
    }
}

fn deserialize_watch_rules<'de, D>(deserializer: D) -> Result<Option<Vec<WatchRule>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Helper {
        List(Vec<WatchRule>),
        Map(std::collections::HashMap<String, String>),
    }

    let helper = Option::<Helper>::deserialize(deserializer)?;
    match helper {
        None => Ok(None),
        Some(Helper::List(list)) => Ok(Some(list)),
        Some(Helper::Map(map)) => {
            let mut list = Vec::new();
            let mut id = 1;
            for (path, target) in map {
                list.push(WatchRule {
                    id,
                    path,
                    target,
                    listen_media: false,
                    auto_delete: None,
                });
                id += 1;
            }
            Ok(Some(list))
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Config {
    pub api_id: Option<i32>,
    pub api_hash: Option<String>,
    pub phone_number: Option<String>,
    pub watch_dir: Option<String>,
    pub auto_delete: Option<bool>,
    pub download_dir: Option<String>,
    pub yt_dlp_path: Option<String>,
    pub yt_dlp_args: Option<String>,
    #[serde(default, deserialize_with = "deserialize_watch_rules")]
    pub watch_rules: Option<Vec<WatchRule>>,
    pub max_concurrent_uploads: Option<usize>,
}

impl Config {
    pub fn get_path() -> PathBuf {
        if let Some(mut home) = dirs::home_dir() {
            home.push(".magebot");
            home.push("config.toml");
            home
        } else {
            PathBuf::from("config.toml")
        }
    }

    pub fn load() -> Self {
        let path = Self::get_path();
        if !path.exists() {
            return Config::default();
        }

        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return Config::default(),
        };

        let mut reader = BufReader::new(file);
        let mut content = String::new();
        if reader.read_to_string(&mut content).is_err() {
            return Config::default();
        }

        toml::from_str(&content).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::get_path();
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }

        let toml_string = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let mut file = File::create(&path)?;
        file.write_all(toml_string.as_bytes())?;
        Ok(())
    }

    pub fn get_download_dir(&self) -> PathBuf {
        if let Some(ref dir) = self.download_dir {
            PathBuf::from(dir)
        } else if let Some(mut home) = dirs::home_dir() {
            home.push(".magebot");
            home.push("downloads");
            home
        } else {
            PathBuf::from("downloads")
        }
    }

    pub fn get_yt_dlp_path(&self) -> String {
        self.yt_dlp_path.clone().unwrap_or_else(|| "yt-dlp".to_string())
    }

    pub fn get_session_path() -> PathBuf {
        if let Some(mut home) = dirs::home_dir() {
            home.push(".magebot");
            home.push("magebot.session");
            home
        } else {
            PathBuf::from("magebot.session")
        }
    }

    pub fn get_watch_rules(&self) -> Vec<WatchRule> {
        let mut rules = self.watch_rules.clone().unwrap_or_default();
        if let Some(ref d) = self.watch_dir {
            if !d.trim().is_empty() && !rules.iter().any(|r| r.path == *d) {
                let next_id = rules.iter().map(|r| r.id).max().unwrap_or(0) + 1;
                rules.push(WatchRule {
                    id: next_id,
                    path: d.clone(),
                    target: "me".to_string(),
                    listen_media: false,
                    auto_delete: None,
                });
            }
        }
        rules
    }

    pub fn add_or_update_watch_rule(&mut self, path: String, target: String) -> WatchRule {
        let mut rules = self.get_watch_rules();
        let rule = if let Some(existing) = rules.iter_mut().find(|r| r.path == path) {
            existing.target = target;
            existing.clone()
        } else {
            let next_id = rules.iter().map(|r| r.id).max().unwrap_or(0) + 1;
            let new_rule = WatchRule {
                id: next_id,
                path: path.clone(),
                target,
                listen_media: false,
                auto_delete: None,
            };
            rules.push(new_rule.clone());
            new_rule
        };

        if self.watch_dir.as_ref() == Some(&path) {
            self.watch_dir = None;
        }
        self.watch_rules = Some(rules);
        rule
    }

    pub fn remove_watch_rule(&mut self, path_or_id: &str) -> bool {
        let mut rules = self.get_watch_rules();
        let trimmed = path_or_id.trim();
        let initial_len = rules.len();

        if let Ok(id) = trimmed.parse::<usize>() {
            rules.retain(|r| r.id != id);
        } else {
            rules.retain(|r| r.path != trimmed);
        }

        if let Some(ref d) = self.watch_dir {
            if d == trimmed {
                self.watch_dir = None;
            }
        }

        let removed = rules.len() < initial_len;
        self.watch_rules = Some(rules);
        removed
    }

    pub fn set_listen_media(&mut self, id: usize, enabled: bool) -> Result<WatchRule, String> {
        let mut rules = self.get_watch_rules();
        if let Some(rule) = rules.iter_mut().find(|r| r.id == id) {
            rule.listen_media = enabled;
            let updated = rule.clone();
            self.watch_rules = Some(rules);
            Ok(updated)
        } else {
            Err(format!("Watch rule with ID {} not found.", id))
        }
    }
}
