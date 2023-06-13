use std::net::SocketAddr;

use clap::{Parser, Subcommand};
use error_stack::{Result, ResultExt};
use thiserror::Error;
use url::Url;

use crate::serve::Config;

mod cache;
mod schema;
mod serve;
mod validate;

#[derive(Debug, Error)]
#[error("application error")]
pub struct Error;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[clap(long, env)]
    kratos_admin_url: Url,

    #[clap(long, env)]
    hydra_admin_url: Url,

    #[clap(long, env)]
    direct_mapping: bool,

    #[clap(long, env, default_value = "indietyp/consent")]
    keyword: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Serve { addr: SocketAddr },
    Validate { schema: String },
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli = Args::parse();

    let config = Config {
        kratos_url: cli.kratos_admin_url,
        hydra_url: cli.hydra_admin_url,
        direct_mapping: cli.direct_mapping,
        keyword: cli.keyword,
    };

    match cli.command {
        Command::Serve { addr } => serve::run(addr, config).await.change_context(Error),
        Command::Validate { schema } => validate::run(schema, config).await.change_context(Error),
    }
}
