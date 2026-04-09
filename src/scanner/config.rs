use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub chat_id: String,
    pub bot_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VkConfig {
    pub user_id: String,
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    pub ip_range: String,
    pub port_range: String,
    pub threads: usize,
    pub masscan_rate: usize,
    pub save_file: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub scanner: ScannerConfig,
    pub telegram: Option<TelegramConfig>,
    pub vk: Option<VkConfig>,
}

impl Config {
    pub fn example() -> Self {
        Self {
            telegram: Some(TelegramConfig {
                chat_id: "your chat id".to_string(),
                bot_token: "your bot token".to_string(),
            }),
            vk: Some(VkConfig {
                user_id: "your user id".to_string(),
                access_token: "your community access token".to_string(),
            }),
            scanner: ScannerConfig {
                save_file: Some("path to your save file".to_string()),
                ..Default::default()
            },
        }
    }
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            ip_range: "192.168.0.1/24".to_string(),
            port_range: "80,443".to_string(),
            threads: 100,
            masscan_rate: 1000,
            save_file: None,
        }
    }
}

impl Config {
    pub fn read_from_file(path: &str) -> anyhow::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        toml::from_str(&data).context("failed to parse config file")
    }
}

