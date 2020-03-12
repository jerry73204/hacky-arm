use failure::Fallible;
use realsense_rust::kind::Format;
use serde::{de::Error, Deserialize, Deserializer};
use std::{
    fs::File,
    io::{prelude::*, BufReader},
    path::{Path, PathBuf},
};

/// The global configuration type.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub dobot_device: PathBuf,
    pub realsense: RealSenseConfig,
    pub object_detector: ObjectDetectorConfig,
    pub visualizer: VisualizerConfig,
}

/// The RealSense configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RealSenseConfig {
    pub depth_camera: DepthCameraConfig,
    pub video_camera: VideoCameraConfig,
}

/// The depth camera configuration on RealSense.
#[derive(Debug, Clone, Deserialize)]
pub struct DepthCameraConfig {
    pub width: usize,
    pub height: usize,
    pub fps: usize,
    #[serde(deserialize_with = "deserialize_format")]
    pub format: Format,
}

/// The video camera configuration on RealSense.
#[derive(Debug, Clone, Deserialize)]
pub struct VideoCameraConfig {
    pub width: usize,
    pub height: usize,
    pub fps: usize,
    #[serde(deserialize_with = "deserialize_format")]
    pub format: Format,
}

/// The RealSense configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ObjectDetectorConfig {
    pub threshold: Option<f64>,
    pub n_dilations: Option<i32>,
    pub n_erosions: Option<i32>,
    pub kernel_size: Option<i32>,
    pub n_objects: Option<usize>,
    pub min_arc_length: Option<f64>,
    pub max_arc_length: Option<f64>,
}

/// The visualizer configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct VisualizerConfig {
    pub enabled: bool,
}

impl Config {
    /// Loads and parses a configuration file.
    pub fn open<P>(path: P) -> Fallible<Self>
    where
        P: AsRef<Path>,
    {
        let mut reader = BufReader::new(File::open(path)?);
        let mut string = String::new();
        reader.read_to_string(&mut string)?;
        let config: Self = json5::from_str(&string)?;
        Ok(config)
    }
}

// This is custom deserializer for Format type.
// See https://serde.rs/field-attrs.html
fn deserialize_format<'de, D>(deserializer: D) -> Result<Format, D::Error>
where
    D: Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    let format = match text.as_str() {
        "Z16" => Format::Z16,
        "RGB8" => Format::Rgb8,
        "RGBA8" => Format::Rgba8,
        "BGR8" => Format::Bgr8,
        "BGRA8" => Format::Bgra8,
        _ => return Err(D::Error::custom(format!("unsupported format {:?}", text))),
    };
    Ok(format)
}
