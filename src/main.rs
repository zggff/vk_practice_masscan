use clap::{Parser, Subcommand};

mod scanner;
use scanner::{config::Config, Scanner};

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// create an example configuration file
    Example {
        /// path to toml example file
        config_path: String,
    },
    /// run program once
    Run {
        /// path to toml example file
        config_path: String,
    },
    /// run program on schedule
    Schedule {
        /// path to toml example file
        config_path: String,

        ///schedule: Cron format string
        schedule: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();
    match args.command {
        Commands::Example { config_path } => {
            let toml = toml::to_string_pretty(&Config::example())?;
            std::fs::write(config_path, toml)?;
        }
        Commands::Run { config_path } => {
            Scanner::from_file(&config_path).await?.run().await?;
        }
        Commands::Schedule {
            config_path,
            schedule,
        } => {
            Scanner::from_file(&config_path)
                .await?
                .run_on_schedule(schedule)
                .await?;
        }
    };
    Ok(())
}
