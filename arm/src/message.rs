use hacky_arm_common::opencv::prelude::Mat;
use hacky_detection::Obj;
use realsense_rust::frame::{marker as frame_marker, Frame};
use std::sync::{Arc, Mutex};

/// Message type produced by RealSense provider.
#[derive(Debug)]
pub struct RealSenseMessage {
    pub depth_frame: Arc<Frame<frame_marker::Depth>>,
    pub color_frame: Arc<Frame<frame_marker::Video>>,
}

/// Message type received by visualizer.
#[derive(Debug)]
pub enum VisualizerMessage {
    RealSenseData {
        depth_frame: Arc<Frame<frame_marker::Depth>>,
        color_frame: Arc<Frame<frame_marker::Video>>,
    },
    ObjectDetection(Mutex<Mat>),
}

/// Message type sent by object detector.
#[derive(Debug, Clone)]
pub struct DetectorMessage {
    pub objects: Vec<Obj>,
}

/// Message type produced by RealSense provider.
#[derive(Debug, Clone)]
pub enum ControlMessage {
    Enter,
}
