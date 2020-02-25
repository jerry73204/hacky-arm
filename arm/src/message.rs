use realsense_rust::frame::{marker as frame_marker, Frame};

/// Message type produced by RealSense provider.
#[derive(Debug)]
pub struct RealSenseMessage {
    pub depth_frame: Frame<frame_marker::Depth>,
    pub color_frame: Frame<frame_marker::Video>,
}

/// Message type received by visualizer.
#[derive(Debug, Clone)]
pub enum VisualizerMessage {
    Dummy,
}
