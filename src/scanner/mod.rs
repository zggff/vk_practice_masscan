use anyhow::Result;
use log::{error, info};
use result::ScanResult;
use std::fmt::Write;
use std::time::Duration;
use tokio_cron_scheduler::{Job, JobScheduler};

pub mod config;
mod notifier;
mod result;

use config::Config;

#[derive(Clone)]
pub struct Scanner {
    config: Config,
}

impl Scanner {
    pub async fn from_file(path: &str) -> Result<Self> {
        let config = Config::read_from_file(path).unwrap_or_default();
        Ok(Self { config })
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

        if let Some(telegram) = &self.config.telegram {
            notifier::send_telegram(&telegram.bot_token, &telegram.chat_id, &message).await?;
        }

        if let Some(vk) = &self.config.vk {
            notifier::send_vk(&vk.access_token, &vk.user_id, &message).await?;
        }

        Ok(())
    }
}
