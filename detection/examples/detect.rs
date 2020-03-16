use argh::FromArgs;
use failure::Fallible;
use hacky_arm_common::opencv::{
    core::{self, Point2f, RotatedRect, Scalar, Size},
    highgui, imgcodecs, imgproc,
    prelude::*,
    videoio::{VideoCapture, CAP_V4L},
};
use hacky_detection::detector::Detector;
use std::os::unix::fs::FileTypeExt;

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

    let file_type = std::fs::metadata(&file)?.file_type();
    dbg!(&file_type);
    if file_type.is_char_device() {
        run_camera(file, detector)?;
    } else if file_type.is_file() {
        run_image(file, detector)?;
    } else {
        panic!("unsupported file type {:?}", file_type);
    }

    Ok(())
}

fn run_camera(path: String, detector: Detector) -> Fallible<()> {
    let mut capture = VideoCapture::from_file(&path, CAP_V4L)?;

    loop {
        let image = {
            let mut image = Mat::default()?;
            capture.read(&mut image)?;
            image
        };
        detect(image, &detector)?;
        let key = highgui::wait_key(10)?;
        if key == 113 {
            break;
        }
    }

    Ok(())
}

fn run_image(path: String, detector: Detector) -> Fallible<()> {
    let image = imgcodecs::imread(&path, imgcodecs::IMREAD_COLOR)?;
    detect(image, &detector)?;
    loop {
        let key = highgui::wait_key(10)?;
        if key == 113 {
            break;
        }
    }
    Ok(())
}

fn detect(mut image: Mat, detector: &Detector) -> Fallible<()> {
    // resize the raw one
    imgproc::resize(
        &image.clone()?,
        &mut image,
        Size {
            width: 640,
            height: 480,
        },
        0.,
        0.,
        imgproc::INTER_LINEAR,
    )?;

    let objects = detector.detect(&mut image)?;
    println!("\n\nResults: {:#?}", objects);

    // visualize the detection
    let window_name = "Detection";
    highgui::named_window(window_name, 0)?;
    highgui::imshow(window_name, &image)?;
    Ok(())
}
