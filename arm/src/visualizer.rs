use crate::{config::Config, message::VisualizerMessage, utils::RateMeter};
use failure::Fallible;
use log::info;
use realsense_rust::{frame::marker as frame_marker, Frame};
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};
use hacky_arm_common::opencv::{highgui, prelude::*};

struct VisualizerCache {
    color_frame: Option<Arc<Frame<frame_marker::Video>>>,
    depth_frame: Option<Arc<Frame<frame_marker::Depth>>>,
    image: Option<Mat>,
}

impl VisualizerCache {
    pub fn new() -> Self {
        Self {
            color_frame: None,
            depth_frame: None,
            image: None,
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

        highgui::named_window("Detection", 0).unwrap();

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
                VisualizerMessage::ObjectDetection(mutex_image) => {
                    let guard = mutex_image.lock().unwrap();
                    let image_mat: &Mat = &*guard;
                    let image: Mat = Mat::clone(image_mat)?;
                    self.cache.image = Some(image);
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
        self.cache.color_frame = Some(color_frame);
        self.cache.depth_frame = Some(depth_frame);
        // let () = color_frame;

        Ok(())
    }

    fn render(&self) -> Fallible<()> {
        // if let Some(color_frame) = &self.cache.color_frame {
        //     // TODO
        //     let color_image = color_frame.image()?;
        //     let color_mat: Mat = HackyTryFrom::try_from(&color_image)?;
        //     highgui::imshow("Color", &color_mat).unwrap();
        // }

        // if let Some(depth_frame) = &self.cache.depth_frame {
        //     // TODO
        //     let depth_image = depth_frame.image()?;
        //     let depth_mat: Mat = HackyTryFrom::try_from(&depth_image)?;
        //     highgui::imshow("Depth", &depth_mat).unwrap();
        // }

        if let Some(image) = &self.cache.image {
            highgui::imshow("Detection", image).unwrap();
        }

        highgui::wait_key(30).unwrap();
        Ok(())
    }
}

/// The handle type that can communicate with visualizer.
#[derive(Debug)]
pub struct VisualizerHandle {
    pub msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
    pub terminate_rx: oneshot::Receiver<Fallible<()>>,
}
