use crate::{
    config::{Config, ObjectDetectorConfig},
    message::{DetectorMessage, RealSenseMessage, VisualizerMessage},
    utils::{HackyTryFrom, RateMeter},
};
use failure::Fallible;
use hacky_arm_common::opencv::{core::Vec3b, prelude::*};
use hacky_detection::Detector;
use log::info;
use realsense_rust::prelude::*;
use std::sync::Arc;
use tokio::{sync::broadcast, task::JoinHandle};

#[derive(Debug)]
pub struct ObjectDetector {
    config: Arc<Config>,
    detector: Arc<Detector>,
    msg_tx: broadcast::Sender<Arc<DetectorMessage>>,
    realsense_msg_rx: broadcast::Receiver<Arc<RealSenseMessage>>,
    viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
}

impl ObjectDetector {
    pub fn start(
        config: Arc<Config>,
        realsense_msg_rx: broadcast::Receiver<Arc<RealSenseMessage>>,
        viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
    ) -> ObjectDetectorHandle {
        let Config {
            object_detector:
                ObjectDetectorConfig {
                    inversion,
                    blur_kernel,
                    n_dilations,
                    dilation_kernel,
                    n_erosions,
                    erosion_kernel,
                    n_objects,
                    min_arc_length,
                    max_arc_length,
                    roi,
                    lower_bound,
                    upper_bound,
                },
            ..
        } = *config;

        let (msg_tx, msg_rx) = broadcast::channel(2);

        let handle = tokio::spawn(async move {
            // init detector
            let detector = {
                let mut detector = Detector::default();
                if let Some(inversion) = inversion {
                    detector.inversion = inversion;
                }
                if let Some(blur_kernel) = blur_kernel {
                    detector.blur_kernel = blur_kernel;
                }
                if let Some(n_dilations) = n_dilations {
                    detector.n_dilations = n_dilations;
                }
                if let Some(dilation_kernel) = dilation_kernel {
                    detector.dilation_kernel = dilation_kernel;
                }
                if let Some(n_erosions) = n_erosions {
                    detector.n_erosions = n_erosions;
                }
                if let Some(erosion_kernel) = erosion_kernel {
                    detector.erosion_kernel = erosion_kernel;
                }
                if let Some(n_objects) = n_objects {
                    detector.n_objects = n_objects;
                }
                if let Some(min_arc_length) = min_arc_length {
                    detector.min_arc_length = min_arc_length;
                }
                if let Some(max_arc_length) = max_arc_length {
                    detector.max_arc_length = max_arc_length;
                }
                if let Some(roi) = roi {
                    detector.roi = roi;
                }
                if let Some(lower_bound) = lower_bound {
                    detector.lower_bound = lower_bound;
                }
                if let Some(upper_bound) = upper_bound {
                    detector.upper_bound = upper_bound;
                }
                Arc::new(detector)
            };

            // start worker
            let provider = Self {
                config,
                detector,
                msg_tx,
                realsense_msg_rx,
                viz_msg_tx,
            };

            provider.run().await?;
            Ok(())
        });

        ObjectDetectorHandle { msg_rx, handle }
    }

    async fn run(mut self) -> Fallible<()> {
        let Config { .. } = &*self.config;
        let mut rate_meter = RateMeter::seconds();

        loop {
            // wait for data from device
            let input_msg = match self.realsense_msg_rx.recv().await {
                Ok(msg) => msg,
                Err(broadcast::RecvError::Lagged(_)) => continue,
                Err(broadcast::RecvError::Closed) => break,
            };
            let detector = Arc::clone(&self.detector);

            // run detection
            // the _blocking_ call is necessary since the detection may take long time
            let (objects, image) = tokio::task::spawn_blocking(move || -> Fallible<_> {
                let RealSenseMessage {
                    color_frame,
                    depth_frame,
                } = &*input_msg;

                let color_image = color_frame.image()?;
                let depth_image = depth_frame.image()?;
                // TODO: handle depth image

                let mut color_mat: Mat = HackyTryFrom::try_from(&color_image)?;
                let objects = detector.detect(&mut color_mat)?;

                Ok((objects, color_mat))
            })
            .await??;

            // send to visualizer
            {
                let msg = VisualizerMessage::ObjectDetection(image.to_vec_2d::<Vec3b>()?);
                let _ = self.viz_msg_tx.send(Arc::new(msg));
            }

            // broadcast message
            {
                let msg = DetectorMessage { objects };
                let _ = self.msg_tx.send(Arc::new(msg));
            }

            if let Some(rate) = rate_meter.tick(1) {
                info!("message rate {} fps", rate);
            }
        }

        info!("object detector finished");
        Ok(())
    }
}

pub struct ObjectDetectorHandle {
    pub msg_rx: broadcast::Receiver<Arc<DetectorMessage>>,
    pub handle: JoinHandle<Fallible<()>>,
}
