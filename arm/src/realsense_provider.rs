use crate::{
    config::{Config, RealSenseConfig},
    message::{RealSenseMessage, VisualizerMessage},
    utils::RateMeter,
};
use failure::Fallible;
use log::{info, warn};
use realsense_rust::{
    frame::marker as frame_marker, Config as RsConfig, Format, Pipeline, StreamKind,
};
use std::sync::Arc;
use tokio::{sync::broadcast, task::JoinHandle};

/// The type instantiates the RealSense provider.
#[derive(Debug)]
pub struct RealSenseProvider {
    config: Arc<Config>,
    msg_tx: broadcast::Sender<Arc<RealSenseMessage>>,
    viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
}

impl RealSenseProvider {
    /// Starts the RealSense provider and returns a handle.
    pub fn start(
        config: Arc<Config>,
        viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
    ) -> RealSenseHandle {
        let (msg_tx, msg_rx) = broadcast::channel(2);

        let handle = tokio::spawn(async {
            let provider = Self {
                config,
                msg_tx,
                viz_msg_tx,
            };
            provider.run().await?;
            Ok(())
        });

        RealSenseHandle { msg_rx, handle }
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
        let mut rate_meter = RateMeter::seconds();

        loop {
            // wait for data from device
            let frames = pipeline.wait_async(None).await?;

            // extract depth and color frames
            let (depth_frame, color_frame) = {
                let mut depth_frame_opt = None;
                let mut color_frame_opt = None;

                for frame_result in frames.try_into_iter()? {
                    let frame_any = frame_result?;
                    let frame_any = match frame_any.try_extend_to::<frame_marker::Depth>()? {
                        Ok(depth_frame) => {
                            depth_frame_opt = Some(depth_frame);
                            continue;
                        }
                        Err(orig_frame) => orig_frame,
                    };
                    let _frame_any = match frame_any.try_extend_to::<frame_marker::Video>()? {
                        Ok(color_frame) => {
                            color_frame_opt = Some(color_frame);
                            continue;
                        }
                        Err(orig_frame) => orig_frame,
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
                (Arc::new(depth_frame), Arc::new(color_frame))
            };

            // send to visualizer
            {
                let msg = VisualizerMessage::RealSenseData {
                    depth_frame: Arc::clone(&depth_frame),
                    color_frame: Arc::clone(&color_frame),
                };
                if let Err(_) = self.viz_msg_tx.send(Arc::new(msg)) {
                    break;
                }
            }

            // broadcast message
            {
                let msg = RealSenseMessage {
                    depth_frame,
                    color_frame,
                };
                let _ = self.msg_tx.send(Arc::new(msg));
            }

            if let Some(rate) = rate_meter.tick(1) {
                info!("message rate {} fps", rate);
            }
        }

        info!("realsense provider finished");
        Ok(())
    }
}

#[derive(Debug)]
pub struct RealSenseHandle {
    pub msg_rx: broadcast::Receiver<Arc<RealSenseMessage>>,
    pub handle: JoinHandle<Fallible<()>>,
}
