use hacky_arm_common::opencv::{
    core::{self, Point, RotatedRect, Size},
    imgproc,
    prelude::*,
    types::VectorOfMat,
};
use failure::Fallible;

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
    pub fn detect(&self, raw: &Mat) -> Fallible<Vec<Obj>> {
        // setup kernel matrix
        let kernel: Mat = imgproc::get_structuring_element(
            imgproc::MORPH_CROSS,
            Size {width: self.kernel_size, height: self.kernel_size},
            Point::new(-1, -1),
        )?;

        // start of image processing
        let mut img = Mat::default()?;

        // - grayscale transformation
        imgproc::cvt_color(&raw, &mut img, imgproc::COLOR_BGR2GRAY, 0)?;

        // - blurring
        imgproc::median_blur(
            &img.clone()?,
            &mut img,
            self.n_blurrings
        )?;

        // - thresholding
        imgproc::threshold(
            &img.clone()?,
            &mut img,
            self.threshold,
            255.,
            imgproc::THRESH_BINARY_INV
        )?;

        // - dilation
        imgproc::dilate(
            &img.clone()?,
            &mut img,
            &kernel,
            Point::new(-1, -1),
            self.n_dilations,
            core::BORDER_CONSTANT,
            imgproc::morphology_default_border_value()?
        )?;

        // - erosion
        imgproc::erode(
            &Mat::clone(&img)?,
            &mut img,
            &kernel,
            Point::new(-1, -1),
            self.n_erosions,
            core::BORDER_CONSTANT,
            imgproc::morphology_default_border_value()?
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

        let results = contours
            .to_vec()
            .into_iter()
            .map(|cnt| {
                let rotated_rect: RotatedRect = imgproc::min_area_rect(&cnt)?;
                let angle: f32 = rotated_rect.angle()?;
                let point: Point = rotated_rect.center()?.to::<i32>().unwrap();
                let arc_len: f64 = imgproc::arc_length(&cnt, true)?;

                // collect all valid detected objects
                if arc_len < 100.0 || arc_len > 1500.0 {
                    return Ok(None);
                }

                let obj = {
                    let Point { x, y } = point;
                    Obj { x, y, angle }
                };
                Ok(Some(obj))
            })
            .filter_map(|result| result.transpose())
            .collect::<Fallible<Vec<_>>>()?;

        Ok(results)
    }
}
