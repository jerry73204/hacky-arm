use crate::{
    config::{Config, RealSenseConfig},
    message::RealSenseMessage,
};
use failure::Fallible;
use log::warn;
use realsense_rust::{
    config::Config as RsConfig, frame::marker as frame_marker, kind::StreamKind, pipeline::Pipeline,
};
use std::{sync::Arc, time::Instant};
use tokio::sync::{broadcast, oneshot};

/// The type instantiates the RealSense provider.
pub struct RealSenseProvider {
    config: Arc<Config>,
    msg_tx: broadcast::Sender<(Instant, Arc<RealSenseMessage>)>,
}

impl RealSenseProvider {
    /// Starts the RealSense provider and returns a handle.
    pub fn start(config: Arc<Config>) -> RealSenseHandle {
        let (terminate_tx, terminate_rx) = oneshot::channel();
        let (msg_tx, msg_rx) = broadcast::channel(2);

        tokio::spawn(async {
            let provider = Self { config, msg_tx };
            let result = provider.run().await;
            let _ = terminate_tx.send(result);
        });

        RealSenseHandle {
            msg_rx,
            terminate_rx,
        }
    }

    async fn run(self) -> Fallible<()> {
        let Config {
            realsense:
                RealSenseConfig {
                    depth_camera,
                    video_camera,
                },
            ..
        } = &*self.config;

        // setup pipeline
        let mut pipeline = {
            let pipeline = Pipeline::new()?;
            let config = RsConfig::new()?
                .enable_stream(
                    StreamKind::Depth,
                    0,
                    depth_camera.width,
                    depth_camera.height,
                    depth_camera.format,
                    depth_camera.fps,
                )?
                .enable_stream(
                    StreamKind::Color,
                    0,
                    video_camera.width,
                    video_camera.height,
                    video_camera.format,
                    video_camera.fps,
                )?;
            pipeline.start_async(Some(config)).await?
        };

        loop {
            // wait for data from device
            let frames = pipeline.wait_async(None).await?;

            // extract depth and color frames
            let (depth_frame, color_frame) = {
                let mut depth_frame_opt = None;
                let mut color_frame_opt = None;

                for frame_result in frames.try_into_iter()? {
                    let frame_any = frame_result?;
                    let frame_any = {
                        let result = frame_any.try_extend_to::<frame_marker::Depth>()?;
                        match result {
                            Ok(depth_frame) => {
                                depth_frame_opt = Some(depth_frame);
                                continue;
                            }
                            Err(orig_frame) => orig_frame,
                        }
                    };
                    let _frame_any = {
                        let result = frame_any.try_extend_to::<frame_marker::Video>()?;
                        match result {
                            Ok(color_frame) => {
                                color_frame_opt = Some(color_frame);
                                continue;
                            }
                            Err(orig_frame) => orig_frame,
                        }
                    };
                }

                let depth_frame = match depth_frame_opt {
                    Some(frame) => frame,
                    None => {
                        warn!("missing depth frame");
                        continue;
                    }
                };
                let color_frame = match color_frame_opt {
                    Some(frame) => frame,
                    None => {
                        warn!("missing color frame");
                        continue;
                    }
                };
                (depth_frame, color_frame)
            };

            // broadcast message
            {
                let msg = RealSenseMessage {
                    depth_frame,
                    color_frame,
                };

                if let Err(_) = self.msg_tx.send((Instant::now(), Arc::new(msg))) {
                    warn!("unable to send message");
                }
            }
        }
    }
}

pub struct RealSenseHandle {
    msg_rx: broadcast::Receiver<(Instant, Arc<RealSenseMessage>)>,
    terminate_rx: oneshot::Receiver<Fallible<()>>,
}

/// The handle type that can communicate with RealSense provider.
impl RealSenseHandle {
    pub fn get_receiver(&mut self) -> &mut broadcast::Receiver<(Instant, Arc<RealSenseMessage>)> {
        &mut self.msg_rx
    }

    pub async fn wait(self) -> Fallible<()> {
        let result = self.terminate_rx.await?;
        result
    }
}
