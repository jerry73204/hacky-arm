use crate::{
    config::{Config, RealSenseConfig},
    message::{RealSenseMessage, VisualizerMessage},
    utils::RateMeter,
};
use failure::Fallible;
use log::info;
use nalgebra::{Point2, Point3};
use realsense_rust::{
    frame::marker as frame_marker, processing_block::marker as processing_block_marker,
    Config as RsConfig, Pipeline, ProcessingBlock, StreamKind,
};
use std::sync::Arc;
use tokio::{sync::broadcast, task::JoinHandle};

/// The type instantiates the RealSense provider.
#[derive(Debug)]
pub struct RealSenseProvider {
    config: Arc<Config>,
    msg_tx: broadcast::Sender<Arc<RealSenseMessage>>,
    viz_msg_tx: broadcast::Sender<VisualizerMessage>,
}

impl RealSenseProvider {
    /// Starts the RealSense provider and returns a handle.
    pub fn start(
        config: Arc<Config>,
        viz_msg_tx: broadcast::Sender<VisualizerMessage>,
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

        // filters
        let mut pointcloud = ProcessingBlock::<processing_block_marker::PointCloud>::create()?;
        let mut aligner =
            ProcessingBlock::<processing_block_marker::Align>::create(StreamKind::Color)?;

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
            let frames = aligner
                .process(frames)?
                .try_extend_to::<frame_marker::Composite>()?
                .unwrap();

            let depth_frame = frames.depth_frame()?.unwrap();
            let color_frame = frames.color_frame()?.unwrap();

            // compute point cloud
            pointcloud.map_to(color_frame.clone())?;
            let points_frame = pointcloud.calculate(depth_frame.clone())?;
            let points = {
                let pts = points_frame
                    .vertices()?
                    .iter()
                    .map(|vertex| {
                        let [x, y, z] = vertex.xyz;
                        Point3::new(x, y, z)
                    })
                    .collect::<Vec<_>>();
                Arc::new(pts)
            };
            let texture_coordinates = points_frame
                .texture_coordinates()?
                .iter()
                .map(|vertex| {
                    let [i, j] = vertex.ij;
                    assert_eq!(std::mem::size_of::<f32>(), std::mem::size_of_val(&i));
                    assert_eq!(std::mem::size_of::<f32>(), std::mem::size_of_val(&j));
                    let x: f32 = unsafe { std::mem::transmute(i) };
                    let y: f32 = unsafe { std::mem::transmute(j) };
                    Point2::new(x, y)
                })
                .collect::<Vec<_>>();

            // send to visualizer
            {
                let msg = VisualizerMessage::RealSenseData {
                    depth_frame: depth_frame.clone(),
                    color_frame: color_frame.clone(),
                    points: Arc::clone(&points),
                    texture_coordinates: texture_coordinates.clone(),
                };
                if let Err(_) = self.viz_msg_tx.send(msg) {
                    break;
                }
            }

            // broadcast message
            {
                let msg = RealSenseMessage {
                    depth_frame,
                    color_frame,
                    points,
                    texture_coordinates,
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
