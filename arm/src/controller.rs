use crate::{
    config::Config,
    message::{ControlMessage, DetectorMessage, DobotMessage, VisualizerMessage},
};
use dobot::Dobot;
use failure::Fallible;
use hacky_detection::Obj;
use log::{info, warn};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
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
    cache: Arc<Mutex<ControllerCache>>,
    enable_auto_grab: Arc<AtomicBool>,
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
            let cache = ControllerCache { detector_msg: None };
            let controller = Controller {
                config,
                detector_msg_rx,
                viz_msg_tx,
                control_rx,
                enable_auto_grab: Arc::new(AtomicBool::from(false)),
                cache: Arc::new(Mutex::new(cache)),
            };
            controller.run().await?;
            Ok(())
        });

        let handle = ControllerHandle { handle };
        Ok(handle)
    }

    async fn run(mut self) -> Fallible<()> {
        // TODO
        let mut dobot_tx = self.start_dobot_worker().await?;
        self.start_auto_grab_worker(dobot_tx.clone())?;

        loop {
            tokio::select! {
                result = self.detector_msg_rx.recv() => {
                    let msg = match result {
                        Ok(msg) => msg,
                        Err(broadcast::RecvError::Lagged(_)) => continue,
                        Err(broadcast::RecvError::Closed) => break,
                    };

                    // self.cache.detector_msg = Some(msg);
                    let mut cache = self.cache.lock().unwrap();
                    cache.detector_msg = Some(msg);
                }
                result = self.control_rx.recv() => {
                    let msg = match result {
                        Ok(msg) => msg,
                        Err(broadcast::RecvError::Lagged(_)) => continue,
                        Err(broadcast::RecvError::Closed) => break,
                    };


                    match msg {
                        ControlMessage::Enter => {
                            self.try_grab_object(&mut dobot_tx).await?;
                        }
                        ControlMessage::Home => {
                            self.try_set_home(&mut dobot_tx)?;
                        }
                        ControlMessage::ToggleAutoGrab => {
                            let prev = self.enable_auto_grab.fetch_xor(true, Ordering::Relaxed);
                            if prev {
                                info!("auto grabbing disbled");
                            } else {
                                info!("auto grabbing enabled");
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn try_grab_object(
        &self,
        dobot_tx: &mut broadcast::Sender<(DobotMessage, Instant)>,
    ) -> Fallible<()> {
        let mut cache = self.cache.lock().unwrap();

        if let Some(msg) = cache.detector_msg.take() {
            match msg.objects.first() {
                Some(obj) => {
                    let dobot_msg = DobotMessage::GrabObject(obj.to_owned());
                    if let Err(_) = dobot_tx.send((dobot_msg, Instant::now())) {
                        return Ok(());
                    }
                }
                None => {
                    warn!("no objects detected");
                }
            }
        }
        Ok(())
    }

    fn try_set_home(
        &self,
        dobot_tx: &mut broadcast::Sender<(DobotMessage, Instant)>,
    ) -> Fallible<()> {
        if let Err(_) = dobot_tx.send((DobotMessage::SetHome, Instant::now())) {
            return Ok(());
        }
        Ok(())
    }

    async fn start_dobot_worker(&self) -> Fallible<broadcast::Sender<(DobotMessage, Instant)>> {
        let mut dobot_tx = {
            let (dobot_tx, mut dobot_rx) = broadcast::channel(1);
            let mut dobot = Dobot::open(&self.config.dobot_device).await?;

            tokio::spawn(async move {
                info!("dobot worker started");
                let mut min_timestamp = Instant::now();

                // dobot.set_home().await?.wait().await?;
                dobot.move_to(220.0, 0.0, 135.0, 9.0).await?.wait().await?;
                loop {
                    let (msg, timestamp) = match dobot_rx.recv().await {
                        Ok(msg) => msg,
                        Err(broadcast::RecvError::Closed) => break,
                        Err(broadcast::RecvError::Lagged(_)) => continue,
                    };

                    if timestamp < Instant::now() - Duration::from_millis(100) {
                        continue;
                    }

                    match msg {
                        DobotMessage::GrabObject(obj) => {
                            let now = Instant::now();
                            if now < min_timestamp {
                                warn!("message is outdated");
                                continue;
                            }

                            let (x, y, angle) = {
                                let Obj { x, y, angle } = obj;
                                let pos_x = (-y + 563) * (275 - 220) / (-345 + 563) + 220;
                                let pos_y = (-x + 765) * (50 - 0) / (-572 + 765) + 0;
                                (pos_x as f32, pos_y as f32, angle)
                            };

                            dobot.release().await?.wait().await?;
                            dobot.move_to(x, y, 70.0, angle + 9.0).await?.wait().await?;
                            dobot
                                .move_to(x, y, -30.0, angle + 9.0)
                                .await?
                                .wait()
                                .await?;
                            dobot.grip().await?.wait().await?;
                            tokio::time::delay_for(Duration::from_secs(1)).await;

                            dobot.move_to(230.0, 9.0, 25.0, 9.0).await?.wait().await?;

                            dobot
                                .move_to(160.0, -165.0, 30.0, 9.0)
                                .await?
                                .wait()
                                .await?;

                            dobot.move_to(0.0, -255.0, -30.0, 9.0).await?.wait().await?;
                            dobot.release().await?.wait().await?;
                            tokio::time::delay_for(Duration::from_secs(1)).await;
                            dobot.move_to(0.0, -255.0, 30.0, 9.0).await?.wait().await?;

                            dobot
                                .move_to(160.0, -165.0, 30.0, 9.0)
                                .await?
                                .wait()
                                .await?;
                            dobot.move_to(230.0, 9.0, 25.0, 9.0).await?.wait().await?;

                            dobot.move_to(220.0, 0.0, 135.0, 9.0).await?.wait().await?;

                            tokio::time::delay_for(Duration::from_secs(2)).await;
                            min_timestamp = Instant::now();
                        }
                        DobotMessage::SetHome => {
                            dobot.set_home().await?.wait().await?;
                            dobot.move_to(220.0, 0.0, 135.0, 9.0).await?.wait().await?;
                        }
                        DobotMessage::Noop(duration) => {
                            tokio::time::delay_for(duration).await;
                        }
                    }
                }

                info!("dobot worker finished");
                Fallible::Ok(())
            });

            dobot_tx
        };

        Ok(dobot_tx)
    }

    fn start_auto_grab_worker(
        &self,
        dobot_tx: broadcast::Sender<(DobotMessage, Instant)>,
    ) -> Fallible<()> {
        let enable_auto_grab = Arc::clone(&self.enable_auto_grab);
        let cache_mutex = Arc::clone(&self.cache);

        tokio::spawn(async move {
            loop {
                tokio::time::delay_for(std::time::Duration::from_millis(100)).await;
                if enable_auto_grab.load(Ordering::Relaxed) {
                    let mut cache = cache_mutex.lock().unwrap();
                    if let Some(msg) = cache.detector_msg.take() {
                        match msg.objects.first() {
                            Some(obj) => {
                                let dobot_msg = DobotMessage::GrabObject(obj.to_owned());
                                if let Err(_) = dobot_tx.send((dobot_msg, Instant::now())) {
                                    break;
                                }
                            }
                            None => {
                                let dobot_msg = DobotMessage::Noop(Duration::from_secs(3));
                                if let Err(_) = dobot_tx.send((dobot_msg, Instant::now())) {
                                    break;
                                }
                                warn!("no objects detected");
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

#[derive(Debug)]
pub struct ControllerHandle {
    pub handle: JoinHandle<Fallible<()>>,
}
