use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod server;
mod types;

#[derive(Debug, Parser)]
struct Cli {
    #[command(flatten)]
    config: Config,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Parser)]
struct Config {
    /// Server TCP port.
    #[arg(short, long, default_value_t = 8765)]
    port: u16,
    /// Server IP address.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
}

#[derive(Debug, Subcommand)]
enum Command {
    Server,
    Client,
}

#[tokio::main]
async fn main() -> Result<()> {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let args = Cli::parse();
    match args.command {
        Command::Server => server::main(args.config).await,
        Command::Client => client::main(args.config).await,
    }
}
