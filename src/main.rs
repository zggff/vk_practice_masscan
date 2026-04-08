use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::scanner::ScanResults;

mod scanner;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ip_range: String,
    pub port_range: String,
    pub threads: usize,
    pub save_file: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ip_range: "192.168.0.1/24".to_string(),
            port_range: "80,443".to_string(),
            threads: 1000,
            save_file: "output.json".to_string(),
        }
    }
}

impl Config {
    fn write_example_file(path: String) -> Result<()> {
        let toml = toml::to_string_pretty(&Config::default())?;
        fs::write(path, toml)?;
        Ok(())
    }
}

async fn run_program(path: String) -> Result<()> {
    let config: Config = if Path::new(&path).exists() {
        let data = fs::read_to_string(path)?;
        toml::from_str(&data)?
    } else {
        Config::default()
    };

    let old_results = if Path::new(&config.save_file).exists() {
        ScanResults::load(&config.save_file)?
    } else {
        ScanResults(Vec::new())
    };

    let results = ScanResults::run_masscan(&config.ip_range, &config.port_range, config.threads)
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

    results.save(&config.save_file)
}

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// create an example configuration file
    Example {
        /// path to whre to save the example config file
        #[arg()]
        filepath: String,
    },
    /// create program
    Run {
        /// path to config file
        #[arg()]
        filepath: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    match args.command {
        Commands::Example { filepath } => Config::write_example_file(filepath),
        Commands::Run { filepath } => run_program(filepath).await,
    }
}
