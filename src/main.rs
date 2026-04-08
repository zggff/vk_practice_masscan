use anyhow::Result;
use clap::{Parser, Subcommand};
use log::{info, error};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::scanner::ScanResult;

mod scanner;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    ip_range: String,
    port_range: String,
    threads: usize,
    masscan_rate: usize,
    save_file: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ip_range: "192.168.0.1/24".to_string(),
            port_range: "80,443".to_string(),
            threads: 100,
            masscan_rate: 1000,
            save_file: "output.json".to_string(),
        }
    }
}

async fn run_once(path: &str) -> Result<()> {
    let config: Config = if Path::new(&path).exists() {
        let data = std::fs::read_to_string(path)?;
        toml::from_str(&data)?
    } else {
        Config::default()
    };

    let old_results = if Path::new(&config.save_file).exists() {
        ScanResult::load_result(&config.save_file)?
    } else {
        ScanResult::default()
    };

    let results = ScanResult::default()
        .populate(&config.ip_range, &config.port_range, config.masscan_rate)
        .await?
        .enrich(config.threads)
        .await;
    for change in results.diff(&old_results) {
        println!("{}:{} at {}", change.ip, change.port, change.protocol);
        if let Some(banner) = change.banner {
            println!("{}", banner)
        }
        println!()
    }

    results.save_result(&config.save_file)
}

#[derive(Parser, Debug)]
struct Args {
    /// path to toml config file
    config_path: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// create an example configuration file
    Example,
    /// run program once
    Run,
    /// run program on schedule
    Schedule {
        ///schedule: Cron format string
        schedule: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();
    match args.command {
        Commands::Example => {
            let toml = toml::to_string_pretty(&Config::default())?;
            std::fs::write(args.config_path, toml)?;
        }
        Commands::Run => {
            run_once(&args.config_path).await?;
        }
        Commands::Schedule { schedule } => {
            let mut sched = JobScheduler::new().await?;
            sched.shutdown_on_ctrl_c();
            sched.set_shutdown_handler(Box::new(|| {
                Box::pin(async move {
                    info!("shutting down");
                })
            }));
            sched
                .add(Job::new_async(&schedule, move |_uuid, mut _l| {
                    let config_path = args.config_path.clone();
                    Box::pin(async move { if let Err(e) = run_once(&config_path).await {
                        error!("scheduled job failed with '{:?}'", e);
                    } })
                })?)
                .await?;
            info!("staring scheduled jobs");
            sched.start().await?;
            loop {
                tokio::time::sleep(Duration::from_mins(10)).await;
            }
        }
    };
    Ok(())
}
