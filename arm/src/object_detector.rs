use crate::{
    config::{Config, ObjectDetectorConfig},
    message::{DetectorMessage, RealSenseMessage, VisualizerMessage},
    utils::{HackyTryFrom, RateMeter},
};
use by_address::ByAddress;
use failure::Fallible;
use geo::{prelude::*, Point};
use hacky_arm_common::opencv::{core::Vec3b, prelude::*};
use hacky_detection::Detector;
use hacky_detection::Obj;
use itertools::Itertools;
use log::info;
use nalgebra::{Point2, Point3};
use realsense_rust::prelude::*;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::{sync::broadcast, task::JoinHandle};

#[derive(Debug)]
pub struct ObjectDetector {
    config: Arc<Config>,
    detector: Arc<Detector>,
    msg_tx: broadcast::Sender<Arc<DetectorMessage>>,
    realsense_msg_rx: broadcast::Receiver<Arc<RealSenseMessage>>,
    viz_msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
}

#[derive(Debug, Clone)]
pub struct Detection {
    pub image: Arc<Vec<Vec<Vec3b>>>,
    pub objects: Vec<Arc<Obj>>,
    pub cloud_to_image_point_correspondences:
        HashMap<ByAddress<Arc<Point3<f32>>>, Arc<Point2<u32>>>,
    pub object_to_cloud_correspondences: HashMap<ByAddress<Arc<Obj>>, Vec<Arc<Point3<f32>>>>,
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
            let detection = tokio::task::spawn_blocking(move || -> Fallible<_> {
                let RealSenseMessage {
                    color_frame,
                    depth_frame,
                    points,
                    texture_coordinates,
                } = &*input_msg;

                // detect objects
                let color_image = color_frame.image()?;
                let depth_image = depth_frame.image()?;
                // let (width, height) = color_image.dimensions();
                let width = color_frame.width()?;
                let height = color_frame.height()?;

                let mut color_mat: Mat = HackyTryFrom::try_from(&color_image)?;

                let objects = detector
                    .detect(&mut color_mat)?
                    .into_iter()
                    .map(|obj| Arc::new(obj))
                    .collect::<Vec<_>>();

                // compute 3D to 2D point correspondences
                let cloud_to_image_point_correspondences = points
                    .iter()
                    .map(Arc::clone)
                    .map(|point| ByAddress(point))
                    .zip(texture_coordinates.iter())
                    .filter_map(|(point3d, texture_coordinate)| {
                        let [x, y]: [_; 2] = texture_coordinate.coords.into();
                        if x >= 0.0 && x < 1.0 && y >= 0.0 && y < 1.0 {
                            let row = (y * height as f32) as u32;
                            let col = (x * width as f32) as u32;
                            let point2d = Arc::new(Point2::new(col, row));
                            Some((point3d, point2d))
                        } else {
                            None
                        }
                    })
                    .collect::<HashMap<_, _>>();

                // compute object to 3d point correspondences
                let object_to_cloud_correspondences = cloud_to_image_point_correspondences
                    .clone()
                    .into_iter()
                    .cartesian_product(objects.iter().map(Arc::clone).map(|obj| ByAddress(obj)))
                    .filter_map(|((point3d, point2d), object)| {
                        let point2d_geo = Point::new(point2d.x as f32, point2d.y as f32);
                        let polygon = &object.polygon;

                        if polygon.contains(&point2d_geo) {
                            Some((object, point3d.0))
                        } else {
                            None
                        }
                    })
                    .into_group_map();

                let image = Arc::new(color_mat.to_vec_2d::<Vec3b>()?);

                let detection = Detection {
                    image,
                    objects,
                    cloud_to_image_point_correspondences,
                    object_to_cloud_correspondences,
                };

                // compute objects and points correspondences
                Ok(Arc::new(detection))
            })
            .await??;

            // send to visualizer
            {
                let msg = VisualizerMessage::ObjectDetection(Arc::clone(&detection));
                let _ = self.viz_msg_tx.send(Arc::new(msg));
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
