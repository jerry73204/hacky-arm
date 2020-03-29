mod config;
mod controller;
mod message;
mod object_detector;
mod processor;
mod realsense_provider;
mod state;
mod utils;
mod visualizer;

use crate::{
    config::Config, controller::Controller, object_detector::ObjectDetector,
    realsense_provider::RealSenseProvider, state::GlobalState, utils::WatchedObject,
    visualizer::Visualizer,
};
use argh::FromArgs;
use failure::Fallible;
use log::info;
use std::{path::PathBuf, sync::Arc};

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

    // init global state
    let state = WatchedObject::new(GlobalState {
        is_dobot_busy: false,
        enable_auto_grab: false,
        termiate: false,
    });

    // {
    //     let state_clone = state.clone();
    //     ctrlc::set_handler(move || {
    //         state_clone.write().termiate = true;
    //         info!("interrupted by user");
    //     })?;
    // }

    // parse arguments
    let args: Args = argh::from_env();
    let Args {
        config: config_path,
    } = args;

    // load config file
    let config = Arc::new(Config::open(config_path)?);

    // start visaulizer
    let visualizer_handle = Visualizer::start(config.clone(), state.clone());

    // start realsense provider
    let realsense_handle =
        RealSenseProvider::start(config.clone(), visualizer_handle.msg_tx.clone());

    // start object detector
    let detector_handle = ObjectDetector::start(
        config.clone(),
        realsense_handle.msg_rx,
        visualizer_handle.msg_tx.clone(),
    );

    // start controller
    let controller_handle = Controller::start(
        config.clone(),
        detector_handle.msg_rx,
        visualizer_handle.msg_tx.clone(),
        visualizer_handle.control_rx,
        state.clone(),
    )
    .await?;

    // wait for workers
    {
        let detector_wait = detector_handle.handle;
        let controller_wait = controller_handle.handle;
        let realsense_wait = realsense_handle.handle;
        let visualizer_wait = visualizer_handle.handle;

        futures::try_join!(
            async move { Fallible::Ok(detector_wait.await??) },
            async move { Fallible::Ok(controller_wait.await??) },
            async move { Fallible::Ok(realsense_wait.await??) },
            async move { Fallible::Ok(visualizer_wait.await??) },
        )?;
    }
    Ok(())
}
