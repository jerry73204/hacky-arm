use crate::{config::Config, message::RealSenseMessage};
use failure::Fallible;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};

pub struct RealSenseProvider {
    config: Arc<Config>,
    msg_tx: broadcast::Sender<RealSenseMessage>,
}

impl RealSenseProvider {
    pub fn start(config: Arc<Config>, channel_size: usize) -> RealSenseHandle {
        let (terminate_tx, terminate_rx) = oneshot::channel();
        let (msg_tx, msg_rx) = broadcast::channel(channel_size);

        tokio::spawn(async {
            let provider = Self { config, msg_tx };
            let _ = terminate_tx.send(provider.run().await);
        });

        RealSenseHandle {
            msg_rx,
            terminate_rx,
        }
    }

    async fn run(mut self) -> Fallible<()> {
        todo!();
        Ok(())
    }
}

pub struct RealSenseHandle {
    msg_rx: broadcast::Receiver<RealSenseMessage>,
    terminate_rx: oneshot::Receiver<Fallible<()>>,
}

impl RealSenseHandle {
    pub fn get_receiver(&mut self) -> &mut broadcast::Receiver<RealSenseMessage> {
        &mut self.msg_rx
    }

    pub async fn wait(self) -> Fallible<()> {
        let result = self.terminate_rx.await?;
        result
    }
}
