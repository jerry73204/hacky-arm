mod config;
mod message;
mod realsense_provider;
mod visualizer;

use crate::{config::Config, message::VisualizerMessage, realsense_provider::RealSenseProvider};
use argh::FromArgs;
use failure::Fallible;
use log::warn;
use std::{path::PathBuf, sync::Arc, time::Instant};
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
    // parse arguments
    let args: Args = argh::from_env();
    let Args {
        config: config_path,
    } = args;

    // load config file
    let config = Arc::new(Config::open(config_path)?);

    // start realsense provider
    let mut realsense_handle = RealSenseProvider::start(Arc::clone(&config));

    // start visaulizer
    let mut visualizer = Visualizer::start(Arc::clone(&config));

    // main loop
    loop {
        // receive data from sensors
        let _msg = match realsense_handle.get_receiver().recv().await {
            Ok(msg) => msg,
            Err(broadcast::RecvError::Closed) => break,
            Err(broadcast::RecvError::Lagged(_)) => continue,
        };

        // send to visualizer
        // TODO: implement visualizer message
        {
            let msg = VisualizerMessage::Dummy;
            let sender = visualizer.get_sender();
            if let Err(_) = sender.send((Instant::now(), Arc::new(msg))) {
                warn!("visualizer is terminated");
                break;
            };
        }
    }

    // wait for workers
    realsense_handle.wait().await?;
    visualizer.wait().await?;

    Ok(())
}
