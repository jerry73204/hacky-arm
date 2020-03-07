use crate::{
    config::Config,
    message::{DetectorMessage, RealSenseMessage, VisualizerMessage},
    utils::RateMeter,
};
use failure::Fallible;
use log::{info, warn};
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};

#[derive(Debug)]
pub struct ObjectDetector {
    config: Arc<Config>,
    msg_tx: broadcast::Sender<Arc<DetectorMessage>>,
    realsense_msg_rx: broadcast::Receiver<Arc<RealSenseMessage>>,
    viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
}

impl ObjectDetector {
    pub fn start(
        config: Arc<Config>,
        realsense_msg_rx: broadcast::Receiver<Arc<RealSenseMessage>>,
        viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
    ) -> ObjectDetectorHandle {
        let (terminate_tx, terminate_rx) = oneshot::channel();
        let (msg_tx, msg_rx) = broadcast::channel(2);

        tokio::spawn(async {
            let provider = Self {
                config,
                msg_tx,
                realsense_msg_rx,
                viz_msg_tx,
            };
            let result = provider.run().await;
            let _ = terminate_tx.send(result);
        });

        ObjectDetectorHandle {
            msg_rx,
            terminate_rx,
        }
    }

    async fn run(mut self) -> Fallible<()> {
        let Config { .. } = &*self.config;
        let mut rate_meter = RateMeter::seconds();

        loop {
            // wait for data from device
            let input_msg = match self.realsense_msg_rx.recv().await {
                Ok(msg) => msg,
                Err(broadcast::RecvError::Lagged(_)) => continue,
                Err(broadcast::RecvError::Closed) => break,
            };

            // TODO: requires hacky-detection

            // send to visualizer
            {
                let msg = VisualizerMessage::ObjectDetection;
                if let Err(_) = self.viz_msg_tx.send(Arc::new(msg)) {
                    break;
                }
            }

            // broadcast message
            {
                let msg = DetectorMessage {};
                if let Err(_) = self.msg_tx.send(Arc::new(msg)) {
                    break;
                }
            }

            if let Some(rate) = rate_meter.tick(1) {
                info!("message rate {} fps", rate);
            }
        }

        Ok(())
    }
}

pub struct ObjectDetectorHandle {
    pub msg_rx: broadcast::Receiver<Arc<DetectorMessage>>,
    pub terminate_rx: oneshot::Receiver<Fallible<()>>,
}
