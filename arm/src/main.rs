mod config;

use crate::config::Config;
use argh::FromArgs;
use failure::Fallible;
use std::path::PathBuf;
use tokio::prelude::*;

#[derive(FromArgs, Debug, Clone)]
/// An arm who learns the arm job.
struct Args {
    #[argh(option, default = "PathBuf::from(\"config.json\")")]
    /// configuration file path.
    pub config: PathBuf,
}

#[tokio::main]
async fn main() -> Fallible<()> {
    // parse arguments
    let args: Args = argh::from_env();
    let Args {
        config: config_path,
    } = args;

    // load config file
    let config = Config::open(config_path);

    Ok(())
}
