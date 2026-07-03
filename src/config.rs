use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;

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
    pub watch_rules: Option<std::collections::HashMap<String, String>>,
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
}
