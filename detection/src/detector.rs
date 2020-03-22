use failure::Fallible;
use geo::{Coordinate, LineString};
use hacky_arm_common::opencv::{
    core::{self, Point, Point2f, RotatedRect, Scalar, Size},
    imgproc,
    prelude::*,
    types::{VectorOfVectorOfPoint, VectorOfi32},
};

#[derive(Debug, Clone)]
pub struct Obj {
    pub x: i32,
    pub y: i32,
    pub angle: f32,
    pub polygon: LineString<f32>,
}

#[derive(Debug)]
pub struct Detector {
    pub inversion: bool,
    pub blur_kernel: i32,
    pub n_dilations: i32,
    pub dilation_kernel: i32,
    pub n_erosions: i32,
    pub erosion_kernel: i32,
    pub n_objects: usize,
    pub min_arc_length: f64,
    pub max_arc_length: f64,
    pub roi: [f64; 2],
    pub lower_bound: [i32; 3],
    pub upper_bound: [i32; 3],
}

impl Default for Detector {
    fn default() -> Self {
        Detector {
            inversion: false,
            blur_kernel: 41,
            n_dilations: 4,
            dilation_kernel: 3,
            n_erosions: 2,
            erosion_kernel: 3,
            n_objects: 10,
            min_arc_length: 94.,
            max_arc_length: 1500.,
            roi: [0.8, 0.8],
            lower_bound: [0, 57, 95],
            upper_bound: [26, 158, 255],
        }
    }
}

impl Detector {
    pub fn detect(&self, raw: &mut Mat) -> Fallible<Vec<Obj>> {
        let to_odd = |value: i32| value.max(3) | 1;

        // start of image processing
        let mut img = Mat::default()?;

        // - HSV threshold
        imgproc::cvt_color(raw, &mut img, imgproc::COLOR_BGR2HSV, 0)?;
        let lower_bound = VectorOfi32::from_iter(self.lower_bound.iter().map(ToOwned::to_owned));
        let upper_bound = VectorOfi32::from_iter(self.upper_bound.iter().map(ToOwned::to_owned));
        core::in_range(&img.clone()?, &lower_bound, &upper_bound, &mut img)?;

        // - blurring
        imgproc::median_blur(&img.clone()?, &mut img, to_odd(self.blur_kernel))?;

        // - inversion
        if self.inversion {
            core::bitwise_not(&img.clone()?, &mut img, &core::no_array()?)?;
        }

        // - dilation
        let dilation_kernel: Mat = imgproc::get_structuring_element(
            imgproc::MORPH_CROSS,
            Size {
                width: to_odd(self.dilation_kernel),
                height: to_odd(self.dilation_kernel),
            },
            Point::new(-1, -1),
        )?;
        imgproc::dilate(
            &img.clone()?,
            &mut img,
            &dilation_kernel,
            Point::new(-1, -1),
            self.n_dilations,
            core::BORDER_CONSTANT,
            imgproc::morphology_default_border_value()?,
        )?;

        // - erosion
        let erosion_kernel: Mat = imgproc::get_structuring_element(
            imgproc::MORPH_CROSS,
            Size {
                width: to_odd(self.erosion_kernel),
                height: to_odd(self.erosion_kernel),
            },
            Point::new(-1, -1),
        )?;
        imgproc::erode(
            &Mat::clone(&img)?,
            &mut img,
            &erosion_kernel,
            Point::new(-1, -1),
            self.n_erosions,
            core::BORDER_CONSTANT,
            imgproc::morphology_default_border_value()?,
        )?;
        // end of image processing

        // find contours
        let contours = {
            let mut contours = VectorOfVectorOfPoint::new();
            imgproc::find_contours(
                &img,
                &mut contours,
                imgproc::RETR_EXTERNAL,
                imgproc::CHAIN_APPROX_SIMPLE,
                Point::default(),
            )?;

            let mut sorted_contours = contours.to_vec();
            sorted_contours.sort_by_cached_key(|cnt| {
                (-1000.0 * imgproc::arc_length(&cnt, true).unwrap()) as i32
            });
            sorted_contours
        };

        let mut rotated_rects = vec![];
        let mut objects = vec![];

        for cnt in contours.iter().take(self.n_objects) {
            let polygon: LineString<_> = cnt
                .iter()
                .map(|point| {
                    let Point { x, y } = point;
                    Coordinate {
                        x: x as f32,
                        y: y as f32,
                    }
                })
                .collect::<Vec<_>>()
                .into();

            // compute rotated rectangle
            let rotated_rect: RotatedRect = imgproc::min_area_rect(&cnt)?;
            let angle: f32 = rotated_rect.angle();
            let point: Point = rotated_rect.center().to::<i32>().unwrap();
            let arc_len: f64 = imgproc::arc_length(&cnt, true)?;

            // collect all valid detected objects
            if arc_len < self.min_arc_length || arc_len > self.max_arc_length {
                continue;
            }

            // reject objects out of roi
            {
                let Size { height, width } = raw.size()?;
                let center_x = width / 2;
                let center_y = height / 2;
                let shift_x = (width as f64 * self.roi[0] / 2.) as i32;
                let shift_y = (height as f64 * self.roi[1] / 2.) as i32;
                let roi_point_1 = (center_x - shift_x, center_y - shift_y);
                let roi_point_2 = (center_x + shift_x, center_y + shift_y);

                let _point = {
                    let Point { x, y } = point.clone();
                    (x, y)
                };

                if _point < roi_point_1 || _point > roi_point_2 {
                    continue;
                }
            }

            // compute rotation angle
            let mut points = vec![Point2f::new(0., 0.); 4];
            rotated_rect.points(points.as_mut())?;

            let compute_norm = |lhs: &Point2f, rhs: &Point2f| {
                let Point2f { x: xl, y: yl } = lhs;
                let Point2f { x: xr, y: yr } = rhs;
                (xl - xr).powi(2) + (yl - yr).powi(2)
            };
            let norm_left_top = compute_norm(&points[0], &points[1]);
            let norm_left_bottom = compute_norm(&points[1], &points[2]);

            let angle = if norm_left_top > norm_left_bottom {
                -angle
            } else {
                -angle - 90.0
            };

            // display rectangle
            for index in 0..4 {
                let next_index = (index + 1) % 4;
                imgproc::line(
                    raw,
                    points[index].to::<i32>().unwrap(),
                    points[next_index].to::<i32>().unwrap(),
                    Scalar::new(0., 255., 0., 0.),
                    3,
                    imgproc::LINE_8,
                    0,
                )?;
            }

            let obj = {
                let Point { x, y } = point;
                Obj {
                    x,
                    y,
                    angle,
                    polygon,
                }
            };

            rotated_rects.push(rotated_rect);
            objects.push(obj);
        }

        // show objects info
        for obj in objects.iter() {
            imgproc::put_text(
                raw,
                &format!("{:?}", obj),
                Point::new(obj.x, obj.y),
                imgproc::FONT_HERSHEY_SIMPLEX,
                0.5,
                Scalar::new(0., 0., 255., 0.),
                1,
                imgproc::LINE_8,
                false,
            )?;
        }

        Ok(objects)
    }
}
