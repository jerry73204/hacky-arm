use realsense_rust::frame::{marker as frame_marker, Frame};
use std::sync::Arc;

/// Message type produced by RealSense provider.
#[derive(Debug)]
pub struct RealSenseMessage {
    pub depth_frame: Arc<Frame<frame_marker::Depth>>,
    pub color_frame: Arc<Frame<frame_marker::Video>>,
}

/// Message type received by visualizer.
#[derive(Debug, Clone)]
pub enum VisualizerMessage {
    RealSenseData {
        depth_frame: Arc<Frame<frame_marker::Depth>>,
        color_frame: Arc<Frame<frame_marker::Video>>,
    },
}
