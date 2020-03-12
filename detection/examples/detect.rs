use argh::FromArgs;
use hacky_detection::detector::Detector;
use failure::Fallible;
use hacky_arm_common::opencv::{
    core::{self, Point2f, RotatedRect, Scalar, Size},
    highgui,
    imgcodecs,
    imgproc,
    prelude::*
};

#[derive(Debug, Clone, FromArgs)]
/// The detection module for hacky-arm project.
struct Args {
    /// input file path.
    #[argh(option, short = 'f', default = "String::from(\"./pic/pen-cap-1.jpg\")")]
    pub file: String,
}

fn main() -> Fallible<()> {
    let args: Args = argh::from_env();
    let Args { file } = args;
    let detector = Detector {
        ..Default::default()
    };

    // get raw image
    let mut raw: Mat = imgcodecs::imread(&file, imgcodecs::IMREAD_COLOR)?;

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

    let objects = detector.detect(&mut raw)?;
    println!("\n\nResults: {:#?}", objects);

    // visualize the detection
    let window_name = "Detection";
    highgui::named_window(window_name, 0)?;
    highgui::imshow(window_name, &raw)?;
    loop {
        let key = highgui::wait_key(10)?;
        if key == 113 {
            break;
        }
    }

    Ok(())
}
