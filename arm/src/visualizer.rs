use crate::{
    config::{Config, VisualizerConfig},
    message::{ControlMessage, VisualizerMessage},
    state::GlobalState,
    utils::{HackyTryFrom, RateMeter, WatchedObject},
};
use crossbeam::channel;
use failure::Fallible;
use hacky_arm_common::opencv::{
    core::{Point, Scalar},
    highgui, imgproc,
    prelude::*,
};
use image::{DynamicImage, GenericImageView, Rgba};
use kiss3d::{
    light::Light,
    window::{State, Window},
};
use log::info;
use nalgebra::{Point2, Point3, Rotation3, Vector3};
use realsense_rust::{frame::marker as frame_marker, prelude::*, Frame};
use std::f32;
use std::sync::Arc;
use tokio::{runtime::Runtime, sync::broadcast, task::JoinHandle};

#[derive(Debug)]
struct PcdVizState {
    rx: channel::Receiver<Vec<(Point3<f32>, Point3<f32>)>>,
    points: Option<Vec<(Point3<f32>, Point3<f32>)>>,
}

impl PcdVizState {
    pub fn new(rx: channel::Receiver<Vec<(Point3<f32>, Point3<f32>)>>) -> Self {
        let state = Self { rx, points: None };
        state
    }
}

impl State for PcdVizState {
    fn step(&mut self, window: &mut Window) {
        // try to receive recent points
        if let Ok(points) = self.rx.try_recv() {
            self.points = Some(points);
        };

        // draw axis
        window.draw_line(
            &Point3::origin(),
            &Point3::new(1.0, 0.0, 0.0),
            &Point3::new(1.0, 0.0, 0.0),
        );
        window.draw_line(
            &Point3::origin(),
            &Point3::new(0.0, 1.0, 0.0),
            &Point3::new(0.0, 1.0, 0.0),
        );
        window.draw_line(
            &Point3::origin(),
            &Point3::new(0.0, 0.0, 1.0),
            &Point3::new(0.0, 0.0, 1.0),
        );

        let axisangle = Vector3::z() * f32::consts::PI;
        let rot = Rotation3::new(axisangle);

        // draw points
        if let Some(points) = &self.points {
            for (position, color) in points.iter() {
                window.draw_point(&(rot * position), color);
            }
        }
    }
}

struct VisualizerCache {
    color_frame: Option<Frame<frame_marker::Video>>,
    depth_frame: Option<Frame<frame_marker::Depth>>,
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
    msg_rx: broadcast::Receiver<VisualizerMessage>,
    control_tx: broadcast::Sender<ControlMessage>,
    pcd_tx: Option<channel::Sender<Vec<(Point3<f32>, Point3<f32>)>>>,
    cache: VisualizerCache,
    state: WatchedObject<GlobalState>,
}

impl Visualizer {
    /// Starts visualizer and returns a handle.
    pub fn start(config: Arc<Config>, state: WatchedObject<GlobalState>) -> VisualizerHandle {
        let (msg_tx, msg_rx) = broadcast::channel(2);
        let (control_tx, control_rx) = broadcast::channel(2);
        let cache = VisualizerCache::new();

        let handle = tokio::spawn(async move {
            let (pcd_tx, pcd_viewer_future) = if config.visualizer.enable_pcd_viewer {
                let (pcd_tx, pcd_rx) = channel::bounded(4);

                let handle = tokio::task::spawn_blocking(move || {
                    let state = PcdVizState::new(pcd_rx);
                    let mut window = Window::new("point cloud");
                    window.set_light(Light::StickToCamera);
                    window.render_loop(state);
                });
                let future = async move { Fallible::Ok(handle.await?) };

                (Some(pcd_tx), Some(future))
            } else {
                (None, None)
            };

            let viz_future = async move {
                tokio::task::spawn_blocking(move || {
                    let visualizer = Self {
                        config,
                        msg_rx,
                        control_tx,
                        pcd_tx,
                        cache,
                        state,
                    };
                    visualizer.run()?;
                    Fallible::Ok(())
                })
                .await??;
                Ok(())
            };

            futures::try_join!(viz_future, futures::future::try_join_all(pcd_viewer_future))?;
            Fallible::Ok(())
        });

        VisualizerHandle {
            msg_tx,
            control_rx,
            handle,
        }
    }

    fn run(mut self) -> Fallible<()> {
        info!("visualizer started");

        let mut runtime = Runtime::new()?;
        let mut rate_meter = RateMeter::seconds();

        loop {
            let msg = match runtime.block_on(self.msg_rx.recv()) {
                Ok(received) => received,
                Err(broadcast::RecvError::Closed) => break,
                Err(broadcast::RecvError::Lagged(_)) => continue,
            };

            match msg {
                VisualizerMessage::RealSenseData {
                    depth_frame,
                    color_frame,
                    points,
                    texture_coordinates,
                } => {
                    self.update_realsense_data(
                        depth_frame.clone(),
                        color_frame.clone(),
                        Arc::clone(&points),
                        texture_coordinates.clone(),
                    )?;
                }
                VisualizerMessage::ObjectDetection(detection) => {
                    let mut image = Mat::from_slice_2d(&detection.image)?;
                    // info!("{:?}", detection.cloud_to_image_point_correspondences);
                    imgproc::put_text(
                        &mut image,
                        "Object Detection Demo",
                        Point::new(5, 45),
                        imgproc::FONT_HERSHEY_SIMPLEX,
                        0.9,
                        Scalar::new(0., 255., 0., 0.),
                        2,
                        imgproc::LINE_8,
                        false,
                    )?;
                    for obj in detection.objects.iter() {
                        imgproc::put_text(
                            &mut image,
                            &format!("({}, {})", obj.x, obj.y),
                            Point::new(obj.x + 30, obj.y - 30),
                            imgproc::FONT_HERSHEY_SIMPLEX,
                            0.5,
                            Scalar::new(0., 0., 255., 0.),
                            1,
                            imgproc::LINE_8,
                            false,
                        )?;
                        imgproc::put_text(
                            &mut image,
                            &format!("angle: {:.1}(deg)", obj.angle),
                            Point::new(obj.x + 30, obj.y - 10),
                            imgproc::FONT_HERSHEY_SIMPLEX,
                            0.5,
                            Scalar::new(0., 0., 255., 0.),
                            1,
                            imgproc::LINE_8,
                            false,
                        )?;
                        imgproc::put_text(
                            &mut image,
                            &format!("depth: {:.2}(m)", obj.depth),
                            Point::new(obj.x + 30, obj.y + 10),
                            imgproc::FONT_HERSHEY_SIMPLEX,
                            0.5,
                            Scalar::new(0., 0., 255., 0.),
                            1,
                            imgproc::LINE_8,
                            false,
                        )?;
                        let n_bricks = ((0.2 - obj.depth) / 0.06 * 7. + 8. - 0.1).round() as i32;
                        imgproc::put_text(
                            &mut image,
                            &format!("bricks: {}", n_bricks),
                            Point::new(obj.x + 30, obj.y + 30),
                            imgproc::FONT_HERSHEY_SIMPLEX,
                            0.5,
                            Scalar::new(0., 0., 255., 0.),
                            1,
                            imgproc::LINE_8,
                            false,
                        )?;
                    }
                    self.cache.image = Some(image);
                }
            }

            let is_dobot_busy = runtime.block_on(self.state.read()).is_dobot_busy;
            // self.render(is_dobot_busy)?;
            self.render(false)?;

            if let Some(rate) = rate_meter.tick(1) {
                info!("message rate {} fps", rate);
            }
        }

        info!("visualizer finished");
        Ok(())
    }

    fn update_realsense_data(
        &mut self,
        depth_frame: Frame<frame_marker::Depth>,
        color_frame: Frame<frame_marker::Video>,
        points: Arc<Vec<Point3<f32>>>,
        texture_coordinates: Vec<Point2<f32>>,
    ) -> Fallible<()> {
        let color_image: DynamicImage = color_frame.image()?.into();
        let (width, height) = color_image.dimensions();

        // construct points with color
        let colored_points = points
            .iter()
            .zip(texture_coordinates.into_iter())
            .map(|(point, texture_coordinate)| {
                let [x, y]: [_; 2] = texture_coordinate.coords.into();
                let color = if x >= 0.0 && x < 1.0 && y >= 0.0 && y < 1.0 {
                    let row = (y * height as f32) as u32;
                    let col = (x * width as f32) as u32;
                    let Rgba([r, g, b, _a]) = color_image.get_pixel(col, row);
                    Point3::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
                } else {
                    Point3::new(0.1, 0.1, 0.1)
                };

                (point.clone(), color)
            })
            .collect::<Vec<_>>();

        // send to point cloud viewer
        if let Some(tx) = &self.pcd_tx {
            let _ = tx.send(colored_points);
        }

        self.cache.color_frame = Some(color_frame);
        self.cache.depth_frame = Some(depth_frame);
        Ok(())
    }

    fn render(&mut self, is_dobot_busy: bool) -> Fallible<()> {
        let VisualizerConfig {
            enable_video_viewer,
            enable_depth_viewer,
            enable_detection_viewer,
            ..
        } = self.config.visualizer;

        if enable_video_viewer && !is_dobot_busy {
            if let Some(color_frame) = &self.cache.color_frame {
                let color_image = color_frame.image()?;
                let color_mat: Mat = HackyTryFrom::try_from(&color_image)?;
                highgui::imshow("Color", &color_mat)?;
            }
        }

        if enable_depth_viewer && !is_dobot_busy {
            if let Some(depth_frame) = &self.cache.depth_frame {
                let depth_image = depth_frame.image()?;
                let depth_mat: Mat = HackyTryFrom::try_from(&depth_image)?;
                let depth_mat = depth_mat
                    .mul(
                        &Mat::ones_size(depth_mat.size()?, depth_mat.typ()?)?.to_mat()?,
                        200.0,
                    )?
                    .to_mat()?;
                highgui::imshow("Depth", &depth_mat).unwrap();
            }
        }

        if enable_detection_viewer && !is_dobot_busy {
            if let Some(image) = &self.cache.image {
                highgui::named_window("Detection", 0)?;
                highgui::imshow("Detection", image)?;
            }
        }

        let key = highgui::wait_key(1)?;
        match key {
            13 => {
                // enter
                info!("Grab!");
                self.control_tx.send(ControlMessage::Enter).unwrap();
            }
            104 => {
                // h
                info!("Set home!");
                self.control_tx.send(ControlMessage::Home).unwrap();
            }
            116 => {
                // t
                info!("Toggle two-facing grab.");
                self.control_tx.send(ControlMessage::Switch).unwrap();
            }
            114 => {
                // r
                info!("Reset!");
                self.control_tx.send(ControlMessage::Reset).unwrap();
            }
            97 => {
                // a
                info!("Auto mode!");
                self.control_tx
                    .send(ControlMessage::ToggleAutoGrab)
                    .unwrap();
            }
            _ => (),
        }

        Ok(())
    }
}

/// The handle type that can communicate with visualizer.
#[derive(Debug)]
pub struct VisualizerHandle {
    pub msg_tx: broadcast::Sender<VisualizerMessage>,
    pub control_rx: broadcast::Receiver<ControlMessage>,
    pub handle: JoinHandle<Fallible<()>>,
}
