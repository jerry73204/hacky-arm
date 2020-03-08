use failure::Fallible;
use hacky_arm_common::opencv::{
    core::{Vec3b, Vec4b},
    imgproc,
    prelude::*,
};
use image::{Bgr, Bgra, Luma, Rgb, Rgba};
use realsense_rust::Rs2Image;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct RateMeter {
    period_ns: u128, // in nanoseconds
    count: usize,
    time: Instant,
}

impl RateMeter {
    pub fn new(period_ns: u128) -> Self {
        Self {
            period_ns,
            count: 0,
            time: Instant::now(),
        }
    }

    pub fn seconds() -> Self {
        Self::new(1_000_000_000)
    }

    pub fn tick(&mut self, count: usize) -> Option<f64> {
        self.count += count;
        let elapsed = self.time.elapsed().as_nanos();

        if elapsed >= self.period_ns {
            let rate = self.count as f64 / elapsed as f64 * 1_000_000_000.0;
            self.count = 0;
            self.time = Instant::now();
            Some(rate)
        } else {
            None
        }
    }
}

pub trait HackyTryFrom<From>
where
    Self: Sized,
{
    type Error;

    fn try_from(from: From) -> Result<Self, Self::Error>;
}

impl<'a> HackyTryFrom<&Rs2Image<'a>> for Mat {
    type Error = failure::Error;

    fn try_from(from: &Rs2Image<'a>) -> Fallible<Self> {
        let mat = match from {
            Rs2Image::Bgr8(image) => {
                let pixel_iter = image.pixels().map(|pixel| {
                    let Bgr(samples) = *pixel;
                    Vec3b::from(samples)
                });
                let mat = Mat::from_exact_iter(pixel_iter)?.reshape(3, image.height() as i32)?;
                mat
            }
            Rs2Image::Bgra8(image) => {
                let pixel_iter = image.pixels().map(|pixel| {
                    let Bgra(samples) = *pixel;
                    Vec4b::from(samples)
                });
                let mat = Mat::from_exact_iter(pixel_iter)?.reshape(4, image.height() as i32)?;
                mat
            }
            Rs2Image::Rgb8(image) => {
                let pixel_iter = image.pixels().map(|pixel| {
                    let Rgb(samples) = *pixel;
                    Vec3b::from(samples)
                });
                let mat = Mat::from_exact_iter(pixel_iter)?.reshape(3, image.height() as i32)?;
                let mat = {
                    let mut out = Mat::default()?;
                    imgproc::cvt_color(&mat, &mut out, imgproc::COLOR_RGB2BGR, 0)?;
                    out
                };
                mat
            }
            Rs2Image::Rgba8(image) => {
                let pixel_iter = image.pixels().map(|pixel| {
                    let Rgba(samples) = *pixel;
                    Vec4b::from(samples)
                });
                let mat = Mat::from_exact_iter(pixel_iter)?.reshape(4, image.height() as i32)?;
                let mat = {
                    let mut out = Mat::default()?;
                    imgproc::cvt_color(&mat, &mut out, imgproc::COLOR_RGBA2BGRA, 0)?;
                    out
                };
                mat
            }
            Rs2Image::Luma16(image) => {
                let pixel_iter = image.pixels().map(|pixel| {
                    let Luma([sample]) = *pixel;
                    sample
                });
                let mat = Mat::from_exact_iter(pixel_iter)?.reshape(1, image.height() as i32)?;
                mat
            }
        };

        Ok(mat)
    }
}

impl<'a> HackyTryFrom<Rs2Image<'a>> for Mat {
    type Error = failure::Error;

    fn try_from(from: Rs2Image<'a>) -> Fallible<Self> {
        HackyTryFrom::try_from(&from)
    }
}
