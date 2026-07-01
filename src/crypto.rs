use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::{thread_rng, RngCore};
use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::PathBuf;

pub fn get_key_path() -> PathBuf {
    if let Some(mut home) = dirs::home_dir() {
        home.push(".magebot");
        home.push("key.bin");
        home
    } else {
        PathBuf::from("key.bin")
    }
}

pub fn get_cookies_toml_path() -> PathBuf {
    if let Some(mut home) = dirs::home_dir() {
        home.push(".magebot");
        home.push("cookies.toml");
        home
    } else {
        PathBuf::from("cookies.toml")
    }
}

pub fn get_encryption_key() -> Result<[u8; 32], String> {
    let path = get_key_path();
    if path.exists() {
        let mut file = File::open(&path).map_err(|e| format!("Failed to open key file: {}", e))?;
        let mut key = [0u8; 32];
        file.read_exact(&mut key)
            .map_err(|e| format!("Failed to read key: {}", e))?;
        Ok(key)
    } else {
        if let Some(parent) = path.parent() {
            let _ = create_dir_all(parent);
        }
        let mut key = [0u8; 32];
        thread_rng().fill_bytes(&mut key);

        let mut file = File::create(&path).map_err(|e| format!("Failed to create key file: {}", e))?;
        file.write_all(&key)
            .map_err(|e| format!("Failed to write key: {}", e))?;
        Ok(key)
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn from_hex(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("Invalid hex length".to_string());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|e| format!("Invalid hex character: {}", e))
        })
        .collect()
}

pub fn encrypt_cookie(plaintext: &str, key: &[u8; 32]) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("Cipher initialization error: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    thread_rng().fill_bytes(&mut nonce_bytes);

    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|e| format!("Encryption error: {}", e))?;

    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(to_hex(&combined))
}

pub fn decrypt_cookie(ciphertext_hex: &str, key: &[u8; 32]) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("Cipher initialization error: {}", e))?;

    let raw_bytes = from_hex(ciphertext_hex)?;
    if raw_bytes.len() < 12 {
        return Err("Ciphertext too short".to_string());
    }

    let (nonce_bytes, encrypted) = raw_bytes.split_at(12);
    let decrypted = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), encrypted)
        .map_err(|e| format!("Decryption error: {}", e))?;

    String::from_utf8(decrypted).map_err(|e| format!("UTF-8 decode error: {}", e))
}

pub fn load_cookies_toml() -> Result<HashMap<String, String>, String> {
    let path = get_cookies_toml_path();
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let mut file = File::open(&path).map_err(|e| format!("Failed to open cookies file: {}", e))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("Failed to read cookies file: {}", e))?;

    toml::from_str(&content).map_err(|e| format!("Failed to parse cookies TOML: {}", e))
}

pub fn save_cookies_toml(map: &HashMap<String, String>) -> Result<(), String> {
    let path = get_cookies_toml_path();
    if let Some(parent) = path.parent() {
        let _ = create_dir_all(parent);
    }

    let toml_string = toml::to_string_pretty(map)
        .map_err(|e| format!("Failed to serialize cookies TOML: {}", e))?;

    let mut file = File::create(&path).map_err(|e| format!("Failed to create cookies file: {}", e))?;
    file.write_all(toml_string.as_bytes())
        .map_err(|e| format!("Failed to write cookies file: {}", e))?;
    Ok(())
}
