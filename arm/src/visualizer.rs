use crate::{
    config::Config,
    message::{ControlMessage, VisualizerMessage},
    utils::RateMeter,
};
use failure::Fallible;
use hacky_arm_common::opencv::{highgui, prelude::*};
use log::info;
use realsense_rust::{frame::marker as frame_marker, Frame};
use std::sync::Arc;
use tokio::{sync::broadcast, task::JoinHandle};

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
    control_tx: broadcast::Sender<ControlMessage>,
    cache: VisualizerCache,
}

impl Visualizer {
    /// Starts visualizer and returns a handle.
    pub fn start(config: Arc<Config>) -> VisualizerHandle {
        let (msg_tx, msg_rx) = broadcast::channel(2);
        let (control_tx, control_rx) = broadcast::channel(2);
        let cache = VisualizerCache::new();

        let handle = tokio::spawn(async {
            let visualizer = Self {
                config,
                msg_rx,
                control_tx,
                cache,
            };
            visualizer.run().await?;
            Ok(())
        });

        VisualizerHandle {
            msg_tx,
            control_rx,
            handle,
        }
    }

    async fn run(mut self) -> Fallible<()> {
        info!("visualizer started");

        let mut rate_meter = RateMeter::seconds();

        highgui::named_window("Detection", 0)?;

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
            highgui::imshow("Detection", image)?;
        }

        let key = highgui::wait_key(30)?;
        match key {
            30 => {
                self.control_tx.send(ControlMessage::Enter).unwrap();
            }
            _ => (),
        }

        Ok(())
    }
}

/// The handle type that can communicate with visualizer.
#[derive(Debug)]
pub struct VisualizerHandle {
    pub msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
    pub control_rx: broadcast::Receiver<ControlMessage>,
    pub handle: JoinHandle<Fallible<()>>,
}
