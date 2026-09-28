use anyhow::Result;
use clap::{Parser, Subcommand};
use ncut_core::{Engine, Status};
use std::{process::ExitCode, time::Duration};

#[derive(Parser)]
#[command(version, about = "NCUT network auto login (no Python required)")]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Check,
    Login(Credentials),
    Run {
        #[command(flatten)]
        credentials: Credentials,
        #[arg(long, default_value_t = 15, value_parser = clap::value_parser!(u64).range(5..=3600))]
        interval: u64,
    },
}
#[derive(clap::Args)]
struct Credentials {
    #[arg(long, env = "NCUT_USERNAME")]
    username: String,
    #[arg(skip)]
    password: String,
}

async fn run() -> Result<bool> {
    let args = Args::parse();
    let engine = Engine::new()?;
    match args.command {
        Command::Check => {
            let status = engine.check().await;
            println!("{}", serde_json::to_string(&status)?);
            Ok(status == Status::Online)
        }
        Command::Login(mut credentials) => {
            credentials.password = std::env::var("NCUT_PASSWORD").unwrap_or_default();
            let status = engine
                .connect(&credentials.username, &credentials.password)
                .await?;
            println!("{}", serde_json::to_string(&status)?);
            Ok(status == Status::Online)
        }
        Command::Run {
            mut credentials,
            interval,
        } => {
            credentials.password = std::env::var("NCUT_PASSWORD").unwrap_or_default();
            anyhow::ensure!(
                !credentials.password.is_empty(),
                "Set NCUT_PASSWORD before running"
            );
            let monitor = async {
                let mut previous = None;
                loop {
                    let mut status = engine.check().await;
                    if status == Status::NeedsLogin {
                        status = engine
                            .connect(&credentials.username, &credentials.password)
                            .await
                            .unwrap_or(Status::LoginFailed);
                    }
                    if previous != Some(status) {
                        println!(
                            "{}",
                            serde_json::to_string(&status).expect("Status serializes")
                        );
                        previous = Some(status);
                    }
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                }
            };
            tokio::select! { _ = monitor => {}, result = shutdown() => { result?; } }
            Ok(true)
        }
    }
}
async fn shutdown() -> Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! { result = tokio::signal::ctrl_c() => { result?; }, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    Ok(())
}
#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(_) => {
            eprintln!("NCUT operation failed; check configuration and network.");
            ExitCode::FAILURE
        }
    }
}
