extern crate clap;
use clap::{Arg, App};
extern crate hacky_arm_common;
use hacky_arm_common::opencv as opencv;
use opencv::{
    core::{self, Mat, Point, Size, Scalar, RotatedRect},
    highgui,
    imgcodecs,
    imgproc,
    types::VectorOfMat,
    prelude::Vector
};

#[derive(Debug)]
struct Obj {
    x: i32,
    y: i32,
    angle: f32,
}


fn run(img_path: &str) -> opencv::Result<Vec<Obj>> {

    // get raw image
    let mut raw: Mat = imgcodecs::imread(&img_path, imgcodecs::IMREAD_COLOR)?;

    // resize the raw one
    imgproc::resize(
        &Mat::clone(&raw)?,
        &mut raw,
        Size { width: 1280, height: 720 },
        0.,
        0.,
        imgproc::INTER_LINEAR
    )?;

    // parameters for erosion/dilation
    let kernel: Mat = imgproc::get_structuring_element(
        imgproc::MORPH_CROSS,
        Size { width: 7, height: 7 },
        Point::new(-1, -1),
    )?;
    let border_value = imgproc::morphology_default_border_value()?;
    let erosion_iteration = 3;
    let dilation_iteration = 3;

    // start of image processing
    let mut img = Mat::default()?;

    // - grayscale transformation
    imgproc::cvt_color(&raw, &mut img, imgproc::COLOR_BGR2GRAY, 0)?;

    // - adjust contrast
    img = img.mul(&1.4, 1.)?.to_mat()?;

    // - erosion
    imgproc::erode(
        &Mat::clone(&img)?,
        &mut img,
        &kernel,
        Point::new(-1, -1),
        erosion_iteration,
        core::BORDER_CONSTANT,
        border_value
    )?;

    // - blurring
    let blurring_iteration = 3;
    imgproc::median_blur(
        &Mat::clone(&img)?,
        &mut img,
        blurring_iteration
    )?;

    // - erosion
    imgproc::dilate(
        &Mat::clone(&img)?,
        &mut img,
        &kernel,
        Point::new(-1, -1),
        dilation_iteration,
        core::BORDER_CONSTANT,
        border_value
    )?;

    // - Canny edge detection
    imgproc::canny(
        &Mat::clone(&img)?,
        &mut img,
        0.,
        255.,
        3,
        true
    )?;

    // - end of image processing

    // find contours
    let mut contours = VectorOfMat::new();
    imgproc::find_contours(
        &img,
        &mut contours,
        imgproc::RETR_EXTERNAL,
        imgproc::CHAIN_APPROX_SIMPLE,
        Point::default()
    )?;

    let mut results = Vec::new();

    for idx in 0..contours.len() {
        let cnt: Mat = contours.get(idx)?;
        let rotated_rect: RotatedRect = imgproc::min_area_rect(&cnt)?;
        let angle: f32 = rotated_rect.angle()?;
        let point: Point = rotated_rect.center()?.to::<i32>().unwrap();
        let arc_len: f64 = imgproc::arc_length(&cnt, true)?;

        // collect all valid detected objects
        if arc_len >= 100.0 && arc_len <= 1500.0 {
            results.push(Obj {
                x: point.x,
                y: point.y,
                angle: angle
            });
        } else {
           continue 
        }

        // display information of each object
        println!("{:02}: angle: {:?}, point: {:?}", idx, angle, point);
        imgproc::put_text(
            &mut raw,
            &format!(
                "Point: ({:.1}, {:.1}), Angle: {:.2}, Len: {:.2}",
                point.x,
                point.y,
                angle,
                arc_len
            ),
            point,
            imgproc::FONT_HERSHEY_SIMPLEX,
            0.5,
            Scalar::new(0., 255., 0., 0.),
            1,
            imgproc::LINE_8,
            false
        )?;

        // draw contours
        let mut cnt_vec = VectorOfMat::new();
        cnt_vec.push(cnt);
        imgproc::draw_contours(
            &mut raw,
            &cnt_vec,
            0,
            Scalar::new(0., 255., 0., 0.),
            3,
            imgproc::LINE_8,
            &Mat::default()?,
            0,
            Point::default()
        )?;
    }

    // visualize the detection
    let window_name = "Detection";
    highgui::named_window(window_name, 0).unwrap();
    highgui::imshow(window_name, &raw).unwrap();
    loop {
        let key = highgui::wait_key(10).unwrap();
        if key == 113 { break; }
    }
    Ok(results)
}

fn main() {
    let args = App::new("sample detection")
        .arg(Arg::with_name("file")
            .short("f")
            .long("file")
            .default_value("./pic/sample-1.jpg")
        )
        .get_matches();
    println!("\n\nResults: {:?}", run(args.value_of("file").unwrap()).unwrap());
}
