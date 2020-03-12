use argh::FromArgs;
use hacky_detection::detector::Detector;
use failure::Fallible;
use serde::Serialize;
use hacky_arm_common::opencv::{
    core::{self, Point2f, RotatedRect, Scalar, Size},
    highgui,
    imgcodecs,
    imgproc,
    prelude::*
};
use std::fs::File;
use std::io::prelude::*;
use log::info;

#[derive(Debug, Clone, FromArgs)]
/// The detection module for hacky-arm project.
struct Args {
    /// input file path.
    #[argh(option, short = 'f', default = "String::from(\"./pic/pen-cap-1.jpg\")")]
    pub file: String,
}

#[derive(Serialize)]
struct Tunable {
    threshold: i32,
    n_dilations: i32,
    n_erosions: i32,
    // kernel_size: i32,
    n_objects: i32,
    min_arc_length: i32,
    max_arc_length: i32,
}

fn main() -> Fallible<()> {
    pretty_env_logger::init();

    let args: Args = argh::from_env();
    let Args { file } = args;
    let mut detector = Detector {
        ..Default::default()
    };


    let window_name = "Detection";
    highgui::named_window(window_name, 0)?;

    // get raw image
    let mut raw: Mat = imgcodecs::imread(&file, imgcodecs::IMREAD_COLOR)?;

    let mut tunable = Tunable {
        threshold: detector.threshold as i32,
        n_dilations: detector.n_dilations,
        n_erosions: detector.n_erosions,
        // kernel_size: detector.kernel_size,
        n_objects: detector.n_objects as i32,
        min_arc_length: detector.min_arc_length as i32,
        max_arc_length: detector.max_arc_length as i32,
    };

    highgui::create_trackbar("threshold", window_name, &mut tunable.threshold, 255, None)?;
    highgui::create_trackbar("n_dilations", window_name, &mut tunable.n_dilations, 20, None)?;
    highgui::create_trackbar("n_erosions", window_name, &mut tunable.n_erosions, 20, None)?;
    // highgui::create_trackbar("kernel_size", window_name, &mut tunable.kernel_size, 20, None)?;
    highgui::create_trackbar("n_objects", window_name, &mut tunable.n_objects, 10, None)?;
    highgui::create_trackbar("min_arc_length", window_name, &mut tunable.min_arc_length, 1000, None)?;
    highgui::create_trackbar("max_arc_length", window_name, &mut tunable.max_arc_length, 3000, None)?;


    // resize the raw one
    imgproc::resize(
        &raw.clone()?,
        &mut raw,
        Size {
            width: 640,
            height: 480,
        },
        0.,
        0.,
        imgproc::INTER_LINEAR,
    )?;


    // visualize the detection
    loop {
        let mut raw_img = raw.clone()?;
        let objects = detector.detect(&mut raw_img)?;
        highgui::imshow(window_name, &raw_img)?;
        info!("\n\nResults: {:#?}", objects);
        let key = highgui::wait_key(10)?;
        if key == 113 {
            break;
        } else if key == 13 {
            let config = serde_json::to_string_pretty(&tunable)?;
            let mut file = File::create("config.json")?;
            write!(&mut file, "{}", config)?;
            println!("\n\n Config saved!");
        } else {
            detector.threshold = tunable.threshold as f64;
            detector.n_dilations = tunable.n_dilations;
            detector.n_erosions = tunable.n_erosions;
            // detector.kernel_size = tunable.kernel_size;
            detector.n_objects = tunable.n_objects as usize;
            detector.min_arc_length = tunable.min_arc_length as f64;
            detector.max_arc_length = tunable.max_arc_length as f64;
        }
    }

    Ok(())
}
