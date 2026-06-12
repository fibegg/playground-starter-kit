use anyhow::Result;
use clap::{Parser, Subcommand};
use rust_axum_react_starter_kit::{config::AppConfig, db, init_tracing, jobs, web};

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve,
    Worker,
    Setup,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let cli = Cli::parse();
    let config = AppConfig::from_env()?;

    match cli.command {
        Command::Serve => web::serve(config).await,
        Command::Worker => jobs::run_worker(config).await,
        Command::Setup => db::setup(config).await,
    }
}
