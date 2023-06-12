// TODO: figment
use std::net::SocketAddr;

use clap::{Parser, Subcommand};
use error_stack::{Result, ResultExt};
use thiserror::Error;

use crate::serve::Config;

mod schema;
mod serve;
mod validate;

#[derive(Debug, Error)]
#[error("application error")]
pub struct Error;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Serve { addr: SocketAddr },
    Validate,
}

fn load_config() -> Config {
    todo!()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli = Args::parse();

    let config = load_config();

    match cli.command {
        Command::Serve { addr } => serve::run(addr, config).await.change_context(Error),
        Command::Validate => validate::run(config).await.change_context(Error),
    }
}
