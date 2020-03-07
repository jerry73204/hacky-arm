mod config;
mod message;
mod processor;
mod realsense_provider;
mod utils;
mod visualizer;

use crate::{config::Config, realsense_provider::RealSenseProvider};
use argh::FromArgs;
use failure::Fallible;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::broadcast;
use visualizer::Visualizer;

#[derive(FromArgs, Debug, Clone)]
/// An arm who learns the arm job.
struct Args {
    #[argh(option, default = "PathBuf::from(\"config.json\")")]
    /// configuration file path.
    pub config: PathBuf,
}

#[tokio::main]
async fn main() -> Fallible<()> {
    pretty_env_logger::init();

    // parse arguments
    let args: Args = argh::from_env();
    let Args {
        config: config_path,
    } = args;

    // load config file
    let config = Arc::new(Config::open(config_path)?);

    // start visaulizer
    let visualizer_handle = Visualizer::start(Arc::clone(&config));

    // start realsense provider
    let mut realsense_handle =
        RealSenseProvider::start(Arc::clone(&config), visualizer_handle.msg_tx.clone());

    // main loop
    loop {
        // receive data from sensors
        let _msg = match realsense_handle.msg_rx.recv().await {
            Ok(msg) => msg,
            Err(broadcast::RecvError::Closed) => break,
            Err(broadcast::RecvError::Lagged(_)) => continue,
        };
    }

    // wait for workers
    realsense_handle.terminate_rx.await??;
    visualizer_handle.terminate_rx.await??;

    Ok(())
}
