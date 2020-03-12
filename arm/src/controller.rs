use crate::{
    config::Config,
    message::{ControlMessage, DetectorMessage, VisualizerMessage},
};
use dobot::Dobot;
use failure::Fallible;
use hacky_detection::Obj;
use log::warn;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

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
    pub async fn start(
        config: Arc<Config>,
        detector_msg_rx: broadcast::Receiver<Arc<DetectorMessage>>,
        viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
        control_rx: broadcast::Receiver<ControlMessage>,
    ) -> Fallible<ControllerHandle> {
        let handle = tokio::spawn(async move {
            let controller = Controller {
                config,
                detector_msg_rx,
                viz_msg_tx,
                control_rx,
                cache: ControllerCache { detector_msg: None },
            };
            controller.run().await?;
            Ok(())
        });

        let handle = ControllerHandle { handle };
        Ok(handle)
    }

    async fn run(mut self) -> Fallible<()> {
        let mut dobot_tx = {
            let (dobot_tx, mut dobot_rx) = mpsc::channel(1);
            let mut dobot = Dobot::open(&self.config.dobot_device).await?;

            tokio::spawn(async move {
                loop {
                    let obj: Obj = match dobot_rx.recv().await {
                        Some(obj) => obj,
                        None => break,
                    };

                    let (x, y) = {
                        let Obj { x, y, .. } = obj;
                        let pos_x = (-y + 563) * (275 - 220) / (-345 + 563) + 220;
                        let pos_y = (-x + 765) * (50 - 0) / (-572 + 765) + 0;
                        (pos_x as f32, pos_y as f32)
                    };

                    dobot.release().await?.wait().await?;
                    dobot.move_to(x, y, 120.0, 9.0).await?.wait().await?;
                    dobot.move_to(x, y, -25.0, 9.0).await?.wait().await?;
                    dobot.grip().await?.wait().await?;
                    tokio::time::delay_for(Duration::from_secs(1)).await;
                    dobot.move_to(220.0, 0.0, 120.0, 9.0).await?.wait().await?;
                    dobot.release().await?.wait().await?;
                }
                Fallible::Ok(())
            });

            dobot_tx
        };

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
                            self.try_invoke_dobot(&mut dobot_tx).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn try_invoke_dobot(&self, dobot_tx: &mut mpsc::Sender<Obj>) -> Fallible<()> {
        if let Some(msg) = &self.cache.detector_msg {
            match msg.objects.first() {
                Some(obj) => match dobot_tx.try_send(obj.to_owned()) {
                    Ok(()) => (),
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        warn!("dobot is busy");
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        return Ok(());
                    }
                },
                None => {
                    warn!("no objects detected");
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
