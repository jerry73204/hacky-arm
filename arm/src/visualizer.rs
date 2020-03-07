use crate::{config::Config, message::VisualizerMessage, utils::RateMeter};
use failure::Fallible;
use image::{ConvertBuffer, DynamicImage, ImageBuffer, Luma};
use log::info;
use realsense_rust::frame::{marker as frame_marker, Frame};
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};

struct VisualizerCache {
    color_image: Option<DynamicImage>,
    depth_image: Option<ImageBuffer<Luma<u8>, Vec<u8>>>,
}

impl VisualizerCache {
    pub fn new() -> Self {
        Self {
            color_image: None,
            depth_image: None,
        }
    }
}

/// The visualizer worker instance.
pub struct Visualizer {
    config: Arc<Config>,
    msg_rx: broadcast::Receiver<Arc<VisualizerMessage>>,
    cache: VisualizerCache,
}

impl Visualizer {
    /// Starts visualizer and returns a handle.
    pub fn start(config: Arc<Config>) -> VisualizerHandle {
        let (terminate_tx, terminate_rx) = oneshot::channel();
        let (msg_tx, msg_rx) = broadcast::channel(2);
        let cache = VisualizerCache::new();

        tokio::spawn(async {
            let visualizer = Self {
                config,
                msg_rx,
                cache,
            };
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

        let mut rate_meter = RateMeter::seconds();

        loop {
            let msg = match self.msg_rx.recv().await {
                Ok(received) => received,
                Err(broadcast::RecvError::Closed) => break,
                Err(broadcast::RecvError::Lagged(_)) => continue,
            };
            match &*msg {
                VisualizerMessage::RealSenseData {
                    depth_frame,
                    color_frame,
                } => {
                    self.update_realsense_data(Arc::clone(depth_frame), Arc::clone(color_frame))?;
                }
            }

            self.render()?;

            if let Some(rate) = rate_meter.tick(1) {
                info!("message rate {} fps", rate);
            }
        }

        info!("visualizer finished");
        Ok(())
    }

    fn update_realsense_data(
        &mut self,
        depth_frame: Arc<Frame<frame_marker::Depth>>,
        color_frame: Arc<Frame<frame_marker::Video>>,
    ) -> Fallible<()> {
        let color_image: DynamicImage = color_frame.color_image()?.into();
        let depth_image: ImageBuffer<Luma<u8>, Vec<u8>> = depth_frame.depth_image()?.convert();

        self.cache.color_image = Some(color_image);
        self.cache.depth_image = Some(depth_image);

        Ok(())
    }

    fn render(&self) -> Fallible<()> {
        if let Some(color_image) = &self.cache.color_image {
            // TODO
        }

        if let Some(depth_image) = &self.cache.depth_image {
            // TODO
        }

        Ok(())
    }
}

/// The handle type that can communicate with visualizer.
#[derive(Debug)]
pub struct VisualizerHandle {
    pub msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
    pub terminate_rx: oneshot::Receiver<Fallible<()>>,
}
