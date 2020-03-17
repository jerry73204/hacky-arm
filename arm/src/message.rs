use hacky_arm_common::opencv::{core::Vec3b, prelude::*};
use hacky_detection::Obj;
use nalgebra::{Point2, Point3};
use realsense_rust::frame::{marker as frame_marker, Frame};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Message type produced by RealSense provider.
#[derive(Debug)]
pub struct RealSenseMessage {
    pub depth_frame: Frame<frame_marker::Depth>,
    pub color_frame: Frame<frame_marker::Video>,
    pub points: Vec<Point3<f32>>,
    pub texture_coordinates: Vec<Point2<f32>>,
}

/// Message type received by visualizer.
#[derive(Debug)]
pub enum VisualizerMessage {
    RealSenseData {
        depth_frame: Frame<frame_marker::Depth>,
        color_frame: Frame<frame_marker::Video>,
        points: Vec<Point3<f32>>,
        texture_coordinates: Vec<Point2<f32>>,
    },
    ObjectDetection(Vec<Vec<Vec3b>>),
}

/// Message type sent by object detector.
#[derive(Debug, Clone)]
pub struct DetectorMessage {
    pub objects: Vec<Obj>,
    pub timestamp: Instant,
}

/// Message type produced by RealSense provider.
#[derive(Debug, Clone)]
pub enum ControlMessage {
    Enter,
    Home,
    ToggleAutoGrab,
}

/// Message type produced by RealSense provider.
#[derive(Debug, Clone)]
pub enum DobotMessage {
    GrabObject(Obj),
    SetHome,
    Noop(Duration),
}
