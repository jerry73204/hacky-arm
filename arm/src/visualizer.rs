use crate::{config::Config, message::VisualizerMessage};
use failure::Fallible;
use log::info;
use std::{sync::Arc, time::Instant};
use tokio::sync::{broadcast, oneshot};

/// The visualizer worker instance.
#[derive(Debug)]
pub struct Visualizer {
    config: Arc<Config>,
    msg_rx: broadcast::Receiver<(Instant, Arc<VisualizerMessage>)>,
}

impl Visualizer {
    /// Starts visualizer and returns a handle.
    pub fn start(config: Arc<Config>) -> VisualizerHandle {
        let (terminate_tx, terminate_rx) = oneshot::channel();
        let (msg_tx, msg_rx) = broadcast::channel(2);

        tokio::spawn(async {
            let visualizer = Self { config, msg_rx };
            let result = visualizer.run().await;
            let _ = terminate_tx.send(result);
        });

        VisualizerHandle {
            msg_tx,
            terminate_rx,
        }
    }

    async fn run(mut self) -> Fallible<()> {
        info!("visualizer started");

        loop {
            let (_, msg) = match self.msg_rx.recv().await {
                Ok(received) => received,
                Err(broadcast::RecvError::Closed) => break,
                Err(broadcast::RecvError::Lagged(_)) => continue,
            };
            match *msg {
                VisualizerMessage::Dummy => (),
            }
        }

        info!("visualizer finished");
        Ok(())
    }
}

/// The handle type that can communicate with visualizer.
#[derive(Debug)]
pub struct VisualizerHandle {
    msg_tx: broadcast::Sender<(Instant, Arc<VisualizerMessage>)>,
    terminate_rx: oneshot::Receiver<Fallible<()>>,
}

impl VisualizerHandle {
    pub fn get_sender(&mut self) -> &mut broadcast::Sender<(Instant, Arc<VisualizerMessage>)> {
        &mut self.msg_tx
    }

    pub async fn wait(self) -> Fallible<()> {
        let result = self.terminate_rx.await?;
        result
    }
}
