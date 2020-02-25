use realsense_rust::frame::{marker as frame_marker, Frame};

#[derive(Debug)]
pub struct RealSenseMessage {
    pub depth_frame: Frame<frame_marker::Depth>,
    pub color_frame: Frame<frame_marker::Video>,
}

#[derive(Debug, Clone)]
pub enum VisualizerMessage {
    Dummy,
}
