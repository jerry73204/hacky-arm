mod config;
mod message;
mod realsense_provider;

use crate::{config::Config, realsense_provider::RealSenseProvider};
use argh::FromArgs;
use failure::Fallible;
use std::{path::PathBuf, sync::Arc};
use tokio::{prelude::*, sync::broadcast};

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
    let config = Arc::new(Config::open(config_path)?);

    let mut realsense_handle = RealSenseProvider::start(Arc::clone(&config), 2);

    loop {
        let msg = match realsense_handle.get_receiver().recv().await {
            Ok(msg) => msg,
            Err(broadcast::RecvError::Closed) => break,
            Err(broadcast::RecvError::Lagged(_)) => continue,
        };
        todo!();
    }

    // wait for workers
    realsense_handle.wait().await?;

    Ok(())
}
