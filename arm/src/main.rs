mod config;
mod message;
mod object_detector;
mod processor;
mod realsense_provider;
mod utils;
mod visualizer;

use crate::{
    config::Config, object_detector::ObjectDetector, realsense_provider::RealSenseProvider,
    visualizer::Visualizer,
};
use argh::FromArgs;
use failure::Fallible;
use lazy_static::lazy_static;
use log::info;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

lazy_static! {
    static ref TERMINATE_FLAG: AtomicBool = AtomicBool::new(false);
}

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

    ctrlc::set_handler(move || {
        TERMINATE_FLAG.store(true, Ordering::SeqCst);
        info!("interrupted by user");
    })?;

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
    let realsense_handle =
        RealSenseProvider::start(Arc::clone(&config), visualizer_handle.msg_tx.clone());

    // start object detector
    let detector_handle = ObjectDetector::start(
        Arc::clone(&config),
        realsense_handle.msg_rx,
        visualizer_handle.msg_tx.clone(),
    );

    // wait for workers
    realsense_handle.terminate_rx.await??;
    detector_handle.terminate_rx.await??;
    visualizer_handle.terminate_rx.await??;

    Ok(())
}
