use crate::{
    config::Config,
    message::{ControlMessage, DetectorMessage, DobotMessage, VisualizerMessage},
    object_detector::Object,
};
use dobot::Dobot;
use failure::Fallible;
use log::{info, warn};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{sync::broadcast, sync::watch, task::JoinHandle};

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
        let spawn_handle = tokio::spawn(async move {
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
        let handle = ControllerHandle {
            handle: spawn_handle,
        };
        Ok(handle)
    }

    async fn run(mut self) -> Fallible<()> {
        let (dobot_handle, mut dobot_tx) = self.start_dobot_worker().await?;
        let dobot_future = async move { Fallible::Ok(dobot_handle.await??) };

        let auto_grab_handle = self.start_auto_grab_worker(dobot_tx.clone())?;
        let auto_grab_future = async move { Fallible::Ok(auto_grab_handle.await??) };

        let loop_future = async move {
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
                            ControlMessage::Reset => {
                                self.try_reset(&mut dobot_tx)?;
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
            Fallible::Ok(())
        };

        futures::try_join!(dobot_future, auto_grab_future, loop_future,)?;

        Ok(())
    }

    async fn try_grab_object(
        &self,
        dobot_tx: &mut broadcast::Sender<(DobotMessage, Instant)>,
    ) -> Fallible<()> {
        let mut cache = self.cache.lock().unwrap();

        if let Some(msg) = cache.detector_msg.take() {
            match msg.detection.objects.first() {
                Some(obj) => {
                    let dobot_msg = DobotMessage::GrabObject(obj.clone());
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

    fn try_reset(&self, dobot_tx: &mut broadcast::Sender<(DobotMessage, Instant)>) -> Fallible<()> {
        let _ = dobot_tx.send((DobotMessage::Reset, Instant::now()));
        Ok(())
    }

    fn try_set_home(
        &self,
        dobot_tx: &mut broadcast::Sender<(DobotMessage, Instant)>,
    ) -> Fallible<()> {
        let _ = dobot_tx.send((DobotMessage::Home, Instant::now()));
        Ok(())
    }

    async fn start_dobot_worker(
        &self,
    ) -> Fallible<(
        JoinHandle<Fallible<()>>,
        broadcast::Sender<(DobotMessage, Instant)>,
    )> {
        let (dobot_tx, mut dobot_rx) = broadcast::channel(1);
        let mut dobot = Dobot::open(&self.config.dobot_device).await?;
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
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

                        // TODO: adjuest position by object distance
                        let [a00, a01, b0, a10, a11, b1] = config.controller.coord_transform;

                        let (x, y, angle, depth) = {
                            let Object {
                                x, y, angle, depth, ..
                            } = *obj;
                            let x = x as f64;
                            let y = y as f64;
                            let pos_x = a00 * x + a01 * y + b0;
                            let pos_y = a10 * x + a11 * y + b1;

                            // let pos_x = (-y + 563) * (275 - 220) / (-345 + 563) + 220;
                            // let pos_x = (-y as f32 + 563.0) * (275.0 - 220.0)
                            //     / (-345.0 + 563.0)
                            //     + 220.0
                            //     + 5.0;
                            // let pos_y = (-x + 765) * (50 - 0) / (-572 + 765) + 0;
                            // let pos_y = (-x + 765) * (50 - 0) / (-572 + 765) + 0 - 10;
                            // let pos_y = (-x as f32 + 765.0) * (50.0 - 0.0) / (-572.0 + 765.0)
                            //     + 0.0
                            //     + 10.0;
                            (pos_x as f32, pos_y as f32, angle, depth)
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
                    DobotMessage::Reset => {
                        dobot.set_home().await?.wait().await?;
                        dobot.move_to(220.0, 0.0, 135.0, 9.0).await?.wait().await?;
                    }
                    DobotMessage::Home => {
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

        Ok((handle, dobot_tx))
    }

    fn start_auto_grab_worker(
        &self,
        dobot_tx: broadcast::Sender<(DobotMessage, Instant)>,
    ) -> Fallible<JoinHandle<Fallible<()>>> {
        let enable_auto_grab = self.enable_auto_grab.clone();
        let cache_mutex = self.cache.clone();

        let handle = tokio::spawn(async move {
            loop {
                // check if auto grab is enabled every a period of time
                tokio::time::delay_for(std::time::Duration::from_millis(100)).await;

                if !enable_auto_grab.load(Ordering::Relaxed) {
                    continue;
                }

                let mut cache = cache_mutex.lock().unwrap();
                if let Some(msg) = cache.detector_msg.take() {
                    match msg.detection.objects.first() {
                        Some(obj) => {
                            let dobot_msg = DobotMessage::GrabObject(obj.clone());
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
            Fallible::Ok(())
        });

        Ok(handle)
    }
}

#[derive(Debug)]
pub struct ControllerHandle {
    pub handle: JoinHandle<Fallible<()>>,
}
