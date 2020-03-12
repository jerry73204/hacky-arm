use failure::Fallible;
use hacky_arm_common::opencv::{
    core::{self, Point, Point2f, Scalar, RotatedRect, Size},
    imgproc,
    prelude::*,
    types::VectorOfMat,
};

#[derive(Debug, Clone)]
pub struct Obj {
    pub x: i32,
    pub y: i32,
    pub angle: f32,
}

#[derive(Debug)]
pub struct Detector {
    pub threshold: f64,
    pub n_dilations: i32,
    pub n_erosions: i32,
    pub n_blurrings: i32,
    pub kernel_size: i32,
}

impl Default for Detector {
    fn default() -> Self {
        Detector {
            threshold: 60.,
            n_dilations: 3,
            n_erosions: 3,
            n_blurrings: 3,
            kernel_size: 3,
        }
    }
}

impl Detector {
    pub fn detect(&self, raw: &mut Mat) -> Fallible<Vec<Obj>> {
        // setup kernel matrix
        let kernel: Mat = imgproc::get_structuring_element(
            imgproc::MORPH_CROSS,
            Size {
                width: self.kernel_size,
                height: self.kernel_size,
            },
            Point::new(-1, -1),
        )?;

        // start of image processing
        let mut img = Mat::default()?;

        // - grayscale transformation
        imgproc::cvt_color(raw, &mut img, imgproc::COLOR_BGR2GRAY, 0)?;

        // - blurring
        imgproc::median_blur(&img.clone()?, &mut img, self.n_blurrings)?;

        // - thresholding
        imgproc::threshold(
            &img.clone()?,
            &mut img,
            self.threshold,
            255.,
            imgproc::THRESH_BINARY_INV,
        )?;

        // - dilation
        imgproc::dilate(
            &img.clone()?,
            &mut img,
            &kernel,
            Point::new(-1, -1),
            self.n_dilations,
            core::BORDER_CONSTANT,
            imgproc::morphology_default_border_value()?,
        )?;

        // - erosion
        imgproc::erode(
            &Mat::clone(&img)?,
            &mut img,
            &kernel,
            Point::new(-1, -1),
            self.n_erosions,
            core::BORDER_CONSTANT,
            imgproc::morphology_default_border_value()?,
        )?;

        // end of image processing

        // find contours
        let mut contours = VectorOfMat::new();
        imgproc::find_contours(
            &img,
            &mut contours,
            imgproc::RETR_EXTERNAL,
            imgproc::CHAIN_APPROX_SIMPLE,
            Point::default(),
        )?;

        let mut rotated_rects = vec![];
        let mut objects = vec![];


        for cnt in contours {
            let rotated_rect: RotatedRect = imgproc::min_area_rect(&cnt)?;
            let angle: f32 = rotated_rect.angle();
            let point: Point = rotated_rect.center().to::<i32>().unwrap();
            let arc_len: f64 = imgproc::arc_length(&cnt, true)?;

            // collect all valid detected objects
            if arc_len < 100.0 || arc_len > 1500.0 {
                continue;
            }

            let obj = {
                let Point { x, y } = point;
                Obj { x, y, angle }
            };

            rotated_rects.push(rotated_rect);
            objects.push(obj);
        }

        for rect in rotated_rects.iter() {
            let mut points = vec![Point2f::new(0., 0.); 4];
            rect.points(points.as_mut())?;
            for index in 0..4 {
                let next_index = (index + 1) % 4;
                // let lhs = &;
                // let rhs = &;
                imgproc::line(
                    raw,
                    points[index].to::<i32>().unwrap(),
                    points[next_index].to::<i32>().unwrap(),
                    Scalar::new(0., 255., 0., 0.),
                    3,
                    imgproc::LINE_8,
                    0
                )?;
            }
        }

        Ok(objects)
    }


   // pub fn plot_box(&self, img: &mut Mat, obj: Vec<Obj>) -> Fallible<Vec<Obj>> {

   // }
}
