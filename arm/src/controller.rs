use crate::{
    config::Config,
    message::{ControlMessage, DetectorMessage, DobotMessage, VisualizerMessage},
    object_detector::Object,
    state::GlobalState,
    utils::WatchedObject,
};
use dobot::Dobot;
use failure::Fallible;
use log::{info, warn};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{sync::broadcast, task::JoinHandle};

#[derive(Debug)]
struct ControllerCache {
    pub detector_msg: Option<Arc<DetectorMessage>>,
}

pub struct Controller {
    config: Arc<Config>,
    detector_msg_rx: broadcast::Receiver<Arc<DetectorMessage>>,
    viz_msg_tx: broadcast::Sender<VisualizerMessage>,
    control_rx: broadcast::Receiver<ControlMessage>,
    cache: Arc<Mutex<ControllerCache>>,
    state: WatchedObject<GlobalState>,
}

impl Controller {
    /// Starts the RealSense provider and returns a handle.
    pub async fn start(
        config: Arc<Config>,
        detector_msg_rx: broadcast::Receiver<Arc<DetectorMessage>>,
        viz_msg_tx: broadcast::Sender<VisualizerMessage>,
        control_rx: broadcast::Receiver<ControlMessage>,
        state: WatchedObject<GlobalState>,
    ) -> Fallible<ControllerHandle> {
        let spawn_handle = tokio::spawn(async move {
            let cache = ControllerCache { detector_msg: None };
            let controller = Controller {
                config,
                detector_msg_rx,
                viz_msg_tx,
                control_rx,
                cache: Arc::new(Mutex::new(cache)),
                state,
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
                            ControlMessage::Switch => {
                                if !self.state.read().await.is_dobot_busy {
                                    let mut state = self.state.write().await;
                                    state.facing = !state.facing;
                                    let _ = dobot_tx.send((DobotMessage::Switch, Instant::now()));
                                }
                            }
                            ControlMessage::ToggleAutoGrab => {
                                let mut state = self.state.write().await;
                                let prev = state.enable_auto_grab;
                                state.enable_auto_grab = !prev;
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

        futures::try_join!(dobot_future, auto_grab_future, loop_future)?;

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
        // let viz_msg_tx = self.viz_msg_tx.clone();
        let (dobot_tx, mut dobot_rx) = broadcast::channel(1);
        let config = self.config.clone();
        let state = self.state.clone();

        let handle = tokio::spawn(async move {
            info!("dobot worker started");
            if config.dobot.enabled {
                let mut dobot = Dobot::open(&config.dobot.device).await?;
                let mut min_timestamp = Instant::now();

                let move_to = |mut dobot: Dobot, facing, x, y, z, r | {
                    async move {
                        if facing {
                            dobot.move_to(x, y, z, r).await?.wait().await?;
                        } else {
                            dobot.move_to(-y, -x, z, r - 90.).await?.wait().await?;
                        }
                        Fallible::Ok(dobot)
                    }
                };

                // move to home
                dobot.move_to(220.0, 0.0, 135.0, 9.0).await?.wait().await?;

                let mut brick_counter = 0;

                loop {
                    state.write().await.is_dobot_busy = false;

                    // wait for next command
                    let (msg, timestamp) = match dobot_rx.recv().await {
                        Ok(msg) => msg,
                        Err(broadcast::RecvError::Closed) => break,
                        Err(broadcast::RecvError::Lagged(_)) => continue,
                    };

                    if timestamp < Instant::now() - Duration::from_millis(100) {
                        continue;
                    }
                    state.write().await.is_dobot_busy = true;

                    let home = (220.0, 0.0, 135.0, 9.0);

                    match msg {
                        DobotMessage::GrabObject(obj) => {
                            let now = Instant::now();
                            if now < min_timestamp {
                                warn!("message is outdated");
                                continue;
                            }

                            let [[a00, a01], [a10, a11]] = config.controller.linear_transform;
                            let [b0, b1] = config.controller.translation;

                            let (x, mut y, angle, depth) = {
                                let Object {
                                    x, y, angle, depth, ..
                                } = *obj;
                                let x = x as f64;
                                let y = y as f64;
                                let pos_x = a00 * x + a01 * y + b0;
                                let pos_y = a10 * x + a11 * y + b1;
                                (pos_x as f32, pos_y as f32, angle, depth)
                            };

                            let depth_range = config.controller.depth_image;
                            let depth_robot = config.controller.depth_robot;

                            let z: f32 = {
                                let mut z: f32 = 0.;
                                for i in 0..(depth_range.len() - 1) {
                                    if depth > (depth_range[i] + depth_range[i + 1]) / 2. {
                                        z = depth_robot[i];
                                        break;
                                    }
                                }
                                if z == 0. {
                                    z = depth_robot[depth_range.len() - 1];
                                }
                                z
                            };

                            let facing = state.read().await.facing;
                            dobot = move_to(dobot, facing, 220.0, 0.0, 135.0, 9.0).await?;

                            if !facing {
                                y = -y;
                            }

                            // move to target position
                            dobot.release().await?.wait().await?;
                            dobot = move_to(dobot, facing, x, y, home.2 - 70., angle + home.3).await?;

                            // go down
                            dobot = move_to(dobot, facing, x, y, z, angle + 9.0).await?;

                            // grip
                            dobot.grip().await?.wait().await?;
                            tokio::time::delay_for(Duration::from_secs(1)).await;

                            // lift up
                            dobot = move_to(dobot, facing, x, y, home.2 - 110., angle + home.3).await?;

                            // rotate 45(deg) clockwisely
                            dobot = move_to(dobot, facing, 196., -160., 50.0, home.3).await?;

                            // rotate 45(deg) clockwisely
                            let x_shift = (brick_counter / 2 - 1) as f32 * 75.;
                            let y_shift = (brick_counter % 2 - 1) as f32 * 60. + 30.;
                            brick_counter = (brick_counter + 1) % 6;
                            let transpose = if facing { -90. } else { 90. };
                            dobot = move_to(dobot, facing, -4. + x_shift, -250. + y_shift, -15., home.3 + transpose).await?;

                            // release
                            dobot.release().await?.wait().await?;
                            tokio::time::delay_for(Duration::from_secs(1)).await;

                            // rotate 45(deg) counterclockwisely
                            dobot = move_to(dobot, facing, 176., -134., 86.0, home.3).await?;

                            // rotate 45(deg) counterclockwisely
                            dobot = move_to(dobot, facing, home.0, home.1, home.2, home.3).await?;

                            // wait for next motion
                            tokio::time::delay_for(Duration::from_secs(2)).await;
                            min_timestamp = Instant::now();
                        }
                        DobotMessage::Reset => {
                            state.write().await.facing = true;
                            dobot.set_home().await?.wait().await?;
                            dobot
                                .move_to(home.0, home.1, home.2, home.3)
                                .await?
                                .wait()
                                .await?;
                        }
                        DobotMessage::Home => {
                            let facing = state.read().await.facing;
                            dobot = move_to(dobot, facing, home.0, home.1, home.2, home.3).await?;
                        }
                        DobotMessage::Switch => {
                            let facing = state.read().await.facing;
                            dobot = move_to(dobot, facing, 196., -160., 50.0, home.3).await?;
                            dobot = move_to(dobot, facing, home.0, home.1, home.2, home.3).await?;
                        }
                        DobotMessage::Noop(duration) => {
                            tokio::time::delay_for(duration).await;
                        }
                    }
                }
            } else {
                info!("Dobot is not enabled. Use simulated dobot controller.");

                loop {
                    state.write().await.is_dobot_busy = false;

                    // wait for next command
                    let (msg, timestamp) = match dobot_rx.recv().await {
                        Ok(msg) => msg,
                        Err(broadcast::RecvError::Closed) => break,
                        Err(broadcast::RecvError::Lagged(_)) => continue,
                    };

                    if timestamp < Instant::now() - Duration::from_millis(100) {
                        continue;
                    }
                    state.write().await.is_dobot_busy = true;

                    match msg {
                        DobotMessage::GrabObject(_obj) => {
                            info!("grab object");
                            tokio::time::delay_for(Duration::from_secs(1)).await;
                        }
                        DobotMessage::Reset => {
                            info!("reset command");
                            tokio::time::delay_for(Duration::from_secs(1)).await;
                        }
                        DobotMessage::Home => {
                            info!("home command");
                            tokio::time::delay_for(Duration::from_secs(1)).await;
                        }
                        DobotMessage::Switch => {
                            info!("switch command");
                            tokio::time::delay_for(Duration::from_secs(1)).await;
                        }
                        DobotMessage::Noop(duration) => {
                            tokio::time::delay_for(duration).await;
                        }
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
        let state = self.state.clone();
        let cache_mutex = self.cache.clone();

        let handle = tokio::spawn(async move {
            let mut counter = 0;
            loop {
                // check if auto grab is enabled every a period of time
                tokio::time::delay_for(std::time::Duration::from_millis(100)).await;

                if !state.read().await.enable_auto_grab {
                    continue;
                }

                let detector_msg = {
                    let mut cache = cache_mutex.lock().unwrap();
                    cache.detector_msg.take()
                };
                if let Some(msg) = detector_msg {
                    match msg.detection.objects.first() {
                        Some(obj) => {
                            counter = 0;
                            let dobot_msg = DobotMessage::GrabObject(obj.clone());
                            if let Err(_) = dobot_tx.send((dobot_msg, Instant::now())) {
                                break;
                            }
                        }
                        None => {
                            if ! state.read().await.is_dobot_busy {
                                counter += 1;
                                let dobot_msg = if counter <= 2 {
                                    DobotMessage::Noop(Duration::from_secs(3))
                                } else {
                                    counter = 0;
                                    let mut writable_state = state.write().await;
                                    writable_state.facing = !writable_state.facing;
                                    DobotMessage::Switch
                                };
                                if let Err(_) = dobot_tx.send((dobot_msg, Instant::now())) {
                                    break;
                                }
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
