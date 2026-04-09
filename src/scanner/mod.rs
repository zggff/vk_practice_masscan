use anyhow::{Context, Result};
use frankenstein::{
    AsyncTelegramApi,
    client_reqwest::Bot,
    methods::{SendMessageParams, VerifyChatParams},
    types::ChatId,
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_cron_scheduler::{Job, JobScheduler};
use std::fmt::Write;
use result::ScanResult;

mod result;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelegramConfig {
    chat_id: ChatId,
    bot_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScannerConfig {
    ip_range: String,
    port_range: String,
    threads: usize,
    masscan_rate: usize,
    save_file: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    scanner: ScannerConfig,
    telegram: Option<TelegramConfig>,
}

impl Config {
    pub fn example() -> Self {
        Self {
            telegram: Some(TelegramConfig {
                chat_id: ChatId::String("your chat id".to_string()),
                bot_token: "your bot token".to_string(),
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
    pub fn read_from_file(path: &str) -> Result<Self> {
        let data = std::fs::read_to_string(path)?;
        toml::from_str(&data).context("failed to parse config file")
    }
}

#[derive(Clone)]
pub struct Scanner {
    config: Config,
    bot: Option<(Bot, ChatId)>,
}

impl Scanner {
    pub async fn from_file(path: &str) -> Result<Self> {
        let config = Config::read_from_file(path).unwrap_or_default();
        let bot = match &config.telegram {
            Some(telegram) => {
                let bot = Bot::new(&telegram.bot_token);
                let verify_chat_params = VerifyChatParams {
                    chat_id: telegram.chat_id.clone(),
                    custom_description: None,
                };
                match bot
                    .verify_chat(&verify_chat_params)
                    .await
                    .map(|res| res.result)
                    .unwrap_or_default()
                {
                    true => {
                        info!("created telegram bot");
                        Some((bot, telegram.chat_id.clone()))
                    }
                    false => {
                        warn!("failed to create telegram bot. Check bot token or chat id");
                        None
                    }
                }
            }
            None => None,
        };
        Ok(Self {
            config,
            bot,
        })
    }

    pub async fn run_on_schedule(self, schedule: String) -> Result<()> {
        let mut sched = JobScheduler::new().await?;
        sched.shutdown_on_ctrl_c();
        sched.set_shutdown_handler(Box::new(|| {
            Box::pin(async move {
                info!("shutting down");
            })
        }));
        sched
            .add(Job::new_async(&schedule, move |_uuid, mut _l| {
                let scanner = self.clone();
                Box::pin(async move {
                    if let Err(e) = scanner.run().await {
                        error!("scheduled job failed with '{:?}'", e);
                    }
                })
            })?)
            .await?;
        info!("staring scheduled jobs");
        sched.start().await?;
        loop {
            tokio::time::sleep(Duration::from_mins(10)).await;
        }
    }

    pub async fn run(&self) -> Result<()> {
        let old_results = self
            .config
            .scanner
            .save_file
            .as_ref()
            .and_then(|file| ScanResult::load_result(file).ok())
            .unwrap_or_default();

        let results = ScanResult::default()
            .populate(
                &self.config.scanner.ip_range,
                &self.config.scanner.port_range,
                self.config.scanner.masscan_rate,
            )
            .await?
            .enrich(self.config.scanner.threads)
            .await;

        let mut message = String::new();
        for change in results.diff(&old_results) {
            writeln!(
                message,
                "{}:{} at {}",
                change.ip, change.port, change.protocol
            )?;
            if let Some(banner) = change.banner {
                writeln!(message, "{}", banner)?;
            }
            writeln!(message)?;
        }

        info!("{}", message);

        if let Some(path) = &self.config.scanner.save_file {
            results.save_result(path)?
        }

        if let Some((bot, id)) = &self.bot {
            let send_message_params = SendMessageParams::builder()
                .chat_id(id.clone())
                .text(message)
                .build();
            bot.send_message(&send_message_params).await?;
        }

        Ok(())
    }
}
