use failure::Fallible;
use realsense_rust::kind::Format;
use serde::{de::Error, Deserialize, Deserializer};
use std::{
    fs::File,
    io::{prelude::*, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub arm_device: PathBuf,
    pub realsense: RealSenseConfig,
    pub visualizer: VisualizerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RealSenseConfig {
    pub depth_camera: DepthCameraConfig,
    pub video_camera: VideoCameraConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepthCameraConfig {
    pub width: usize,
    pub height: usize,
    pub fps: usize,
    #[serde(deserialize_with = "deserialize_format")]
    pub format: Format,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VideoCameraConfig {
    pub width: usize,
    pub height: usize,
    pub fps: usize,
    #[serde(deserialize_with = "deserialize_format")]
    pub format: Format,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VisualizerConfig {
    pub enabled: bool,
}

impl Config {
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
