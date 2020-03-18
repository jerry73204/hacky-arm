use crate::{
    config::Config,
    message::{ControlMessage, VisualizerMessage},
    utils::{HackyTryFrom, RateMeter},
};
use crossbeam::channel;
use failure::Fallible;
use hacky_arm_common::opencv::{
    core::{Point, Scalar},
    highgui, imgcodecs, imgproc,
    prelude::*,
    types::VectorOfi32,
};
use image::{DynamicImage, GenericImageView, Rgba};
use kiss3d::{
    light::Light,
    window::{State, Window},
};
use log::info;
use nalgebra::{Point2, Point3, Vector3, Rotation3};
use std::f32;
use realsense_rust::{frame::marker as frame_marker, prelude::*, Frame};
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
                window.draw_point(&(rot * position), &(rot * color));
            }
        }
    }
}

struct VisualizerCache {
    color_frame: Option<Frame<frame_marker::Video>>,
    depth_frame: Option<Frame<frame_marker::Depth>>,
    image: Option<Mat>,
    points: Option<Vec<(Point3<f32>, Point3<f32>)>>,
}

impl VisualizerCache {
    pub fn new() -> Self {
        Self {
            color_frame: None,
            depth_frame: None,
            image: None,
            points: None,
        }
    }
}

/// The visualizer worker instance.
pub struct Visualizer {
    config: Arc<Config>,
    msg_rx: broadcast::Receiver<Arc<VisualizerMessage>>,
    control_tx: broadcast::Sender<ControlMessage>,
    pcd_tx: channel::Sender<Vec<(Point3<f32>, Point3<f32>)>>,
    cache: VisualizerCache,
}

impl Visualizer {
    /// Starts visualizer and returns a handle.
    pub fn start(config: Arc<Config>) -> VisualizerHandle {
        let (msg_tx, msg_rx) = broadcast::channel(2);
        let (control_tx, control_rx) = broadcast::channel(2);
        let cache = VisualizerCache::new();

        let handle = tokio::spawn(async {
            let pcd_tx = {
                let (pcd_tx, pcd_rx) = channel::bounded(4);

                std::thread::spawn(move || {
                    let state = PcdVizState::new(pcd_rx);
                    let mut window = Window::new("point cloud");
                    window.set_light(Light::StickToCamera);
                    window.render_loop(state);
                });
                pcd_tx
            };

            let visualizer = Self {
                config,
                msg_rx,
                control_tx,
                pcd_tx,
                cache,
            };

            tokio::task::spawn_blocking(|| visualizer.run()).await??;
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
            match &*msg {
                VisualizerMessage::RealSenseData {
                    depth_frame,
                    color_frame,
                    points,
                    texture_coordinates,
                } => {
                    self.update_realsense_data(
                        depth_frame.clone(),
                        color_frame.clone(),
                        points.clone(),
                        texture_coordinates.clone(),
                    )?;
                }
                VisualizerMessage::ObjectDetection(bytes) => {
                    let mut image = Mat::from_slice_2d(&bytes)?;
                    imgproc::put_text(
                        &mut image,
                        "物件偵測",
                        Point::new(0, 0),
                        imgproc::FONT_HERSHEY_SIMPLEX,
                        0.5,
                        Scalar::new(0., 255., 0., 0.),
                        1,
                        imgproc::LINE_8,
                        false,
                    )?;
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
        depth_frame: Frame<frame_marker::Depth>,
        color_frame: Frame<frame_marker::Video>,
        points: Vec<Point3<f32>>,
        texture_coordinates: Vec<Point2<f32>>,
    ) -> Fallible<()> {
        let color_image: DynamicImage = color_frame.image()?.into();
        let (width, height) = color_image.dimensions();

        let colored_points = points
            .into_iter()
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

                (point, color)
            })
            .collect::<Vec<_>>();

        let _ = self.pcd_tx.send(colored_points);

        self.cache.color_frame = Some(color_frame);
        self.cache.depth_frame = Some(depth_frame);
        Ok(())
    }

    fn render(&self) -> Fallible<()> {
        highgui::named_window("Detection", 0)?;

        if let Some(color_frame) = &self.cache.color_frame {
            let color_image = color_frame.image()?;
            let color_mat: Mat = HackyTryFrom::try_from(&color_image)?;
            highgui::imshow("Color", &color_mat)?;
        }

        if let Some(depth_frame) = &self.cache.depth_frame {
            let depth_image = depth_frame.image()?;
            let depth_mat: Mat = HackyTryFrom::try_from(&depth_image)?;
            highgui::imshow("Depth", &depth_mat).unwrap();
        }

        if let Some(image) = &self.cache.image {
            highgui::imshow("Detection", image)?;
        }

        let key = highgui::wait_key(1)?;
        match key {
            13 => {
                // enter
                self.control_tx.send(ControlMessage::Enter).unwrap();
            }
            104 => {
                // h
                self.control_tx.send(ControlMessage::Home).unwrap();
            }
            97 => {
                // a
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
    pub msg_tx: broadcast::Sender<Arc<VisualizerMessage>>,
    pub control_rx: broadcast::Receiver<ControlMessage>,
    pub handle: JoinHandle<Fallible<()>>,
}
