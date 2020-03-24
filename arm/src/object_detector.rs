use crate::{
    config::{Config, ObjectDetectorConfig},
    message::{DetectorMessage, RealSenseMessage, VisualizerMessage},
    utils::{HackyTryFrom, RateMeter},
};
use failure::Fallible;
use geo::LineString;
use hacky_arm_common::opencv::{
    core::{Point, Scalar, Vec3b},
    imgproc,
    prelude::*,
};
use hacky_detection::Detector;
use hacky_detection::Obj;
use log::info;
use realsense_rust::prelude::*;
use std::{sync::Arc, time::Instant};
use tokio::{sync::broadcast, task::JoinHandle};

#[derive(Debug)]
pub struct ObjectDetector {
    config: Arc<Config>,
    detector: Arc<Detector>,
    msg_tx: broadcast::Sender<Arc<DetectorMessage>>,
    realsense_msg_rx: broadcast::Receiver<Arc<RealSenseMessage>>,
    viz_msg_tx: broadcast::Sender<VisualizerMessage>,
}

#[derive(Debug, Clone)]
pub struct Detection {
    pub image: Arc<Vec<Vec<Vec3b>>>,
    pub objects: Vec<Arc<Object>>,
}

#[derive(Debug, Clone)]
pub struct Object {
    pub x: i32,
    pub y: i32,
    pub angle: f32,
    pub polygon: LineString<f32>,
    pub depth: f32,
}

impl ObjectDetector {
    pub fn start(
        config: Arc<Config>,
        realsense_msg_rx: broadcast::Receiver<Arc<RealSenseMessage>>,
        viz_msg_tx: broadcast::Sender<VisualizerMessage>,
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

                // turn off position drawing, move it to visualizer
                detector.draw_position = false;

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
        let mut rate_meter = RateMeter::seconds();

        loop {
            // wait for data from device
            let input_msg = match self.realsense_msg_rx.recv().await {
                Ok(msg) => msg,
                Err(broadcast::RecvError::Lagged(_)) => continue,
                Err(broadcast::RecvError::Closed) => break,
            };
            let detector = self.detector.clone();

            // run detection
            // the _blocking_ call is necessary since the detection may take long time
            let detection = tokio::task::spawn(async move {
                let RealSenseMessage {
                    color_frame,
                    depth_frame,
                    ..
                } = &*input_msg;

                // detect objects
                let color_image = color_frame.image()?;
                let mut color_mat: Mat = HackyTryFrom::try_from(&color_image)?;

                let objects2d = detector.detect(&mut color_mat)?;

                // get distance of each object
                let objects = objects2d
                    .into_iter()
                    .map(|obj| {
                        let Obj {
                            x,
                            y,
                            angle,
                            polygon,
                        } = obj;
                        let distance = depth_frame.distance(x as usize, y as usize)?;
                        // imgproc::put_text(
                        //     &mut color_mat,
                        //     &format!("depth: {:.2}(m)", distance),
                        //     Point::new(x + 20, y + 40),
                        //     imgproc::FONT_HERSHEY_SIMPLEX,
                        //     0.5,
                        //     Scalar::new(0., 0., 255., 0.),
                        //     1,
                        //     imgproc::LINE_8,
                        //     false,
                        // )?;
                        let object = Object {
                            x,
                            y,
                            angle,
                            polygon,
                            depth: distance,
                        };
                        Ok(Arc::new(object))
                    })
                    .collect::<Fallible<Vec<_>>>()?;

                let image = Arc::new(color_mat.to_vec_2d::<Vec3b>()?);

                let detection = Detection { image, objects };

                // compute objects and points correspondences
                Fallible::Ok(Arc::new(detection))
            })
            .await??;

            // send to visualizer
            {
                let msg = VisualizerMessage::ObjectDetection(Arc::clone(&detection));
                let _ = self.viz_msg_tx.send(msg);
            }

            // broadcast message
            {
                let msg = DetectorMessage {
                    detection,
                    timestamp: Instant::now(),
                };
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
