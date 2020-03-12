use crate::{
    config::Config,
    message::{ControlMessage, DetectorMessage, VisualizerMessage},
};
use failure::Fallible;
use std::sync::Arc;
use tokio::{sync::broadcast, task::JoinHandle};

struct ControllerCache {
    pub detector_msg: Option<Arc<DetectorMessage>>,
}

pub struct Controller {
    config: Arc<Config>,
    detector_msg_rx: broadcast::Receiver<Arc<DetectorMessage>>,
    viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
    control_rx: broadcast::Receiver<ControlMessage>,
    cache: ControllerCache,
}

impl Controller {
    /// Starts the RealSense provider and returns a handle.
    pub fn start(
        config: Arc<Config>,
        detector_msg_rx: broadcast::Receiver<Arc<DetectorMessage>>,
        viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
        control_rx: broadcast::Receiver<ControlMessage>,
    ) -> ControllerHandle {
        let handle = tokio::spawn(async move {
            let cache = ControllerCache { detector_msg: None };
            let controller = Controller {
                config,
                detector_msg_rx,
                viz_msg_tx,
                control_rx,
                cache,
            };
            controller.run().await?;
            Ok(())
        });

        ControllerHandle { handle }
    }

    async fn run(mut self) -> Fallible<()> {
        loop {
            tokio::select! {
                result = self.detector_msg_rx.recv() => {
                    let msg = match result {
                        Ok(msg) => msg,
                        Err(broadcast::RecvError::Lagged(_)) => continue,
                        Err(broadcast::RecvError::Closed) => break,
                    };

                    self.cache.detector_msg = Some(msg);
                }
                result = self.control_rx.recv() => {
                    let msg = match result {
                        Ok(msg) => msg,
                        Err(broadcast::RecvError::Lagged(_)) => continue,
                        Err(broadcast::RecvError::Closed) => break,
                    };

                    match msg {
                        ControlMessage::Enter => {
                            // TODO
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct ControllerHandle {
    pub handle: JoinHandle<Fallible<()>>,
}
