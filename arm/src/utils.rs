use failure::Fallible;
use hacky_arm_common::opencv::{
    core::{Vec3b, Vec4b},
    imgproc,
    prelude::*,
};
use image::{Bgr, Bgra, Luma, Rgb, Rgba};
use realsense_rust::Rs2Image;
use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::{watch, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug)]
pub struct WatchedObject<T> {
    tx: Arc<Mutex<watch::Sender<()>>>,
    rx: watch::Receiver<()>,
    object: Arc<RwLock<T>>,
}

impl<T> Clone for WatchedObject<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            rx: self.rx.clone(),
            object: self.object.clone(),
        }
    }
}

impl<T> WatchedObject<T> {
    pub fn new(init: T) -> Self {
        let (tx, rx) = watch::channel(());
        let object = Arc::new(RwLock::new(init));
        Self {
            tx: Arc::new(Mutex::new(tx)),
            rx,
            object,
        }
    }

    pub async fn write<'a>(&'a self) -> UpdateHandle<'a, T> {
        UpdateHandle {
            obj: self,
            lock: self.object.write().await,
        }
    }

    pub async fn read<'a>(&'a self) -> RwLockReadGuard<'a, T> {
        self.object.read().await
    }

    pub async fn watch<'a>(&'a mut self) -> Option<()> {
        self.rx.recv().await
    }
}

#[derive(Debug)]
pub struct UpdateHandle<'a, T> {
    obj: &'a WatchedObject<T>,
    lock: RwLockWriteGuard<'a, T>,
}

impl<'a, T> Deref for UpdateHandle<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.lock.deref()
    }
}

impl<'a, T> DerefMut for UpdateHandle<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.lock.deref_mut()
    }
}

impl<'a, T> Drop for UpdateHandle<'a, T> {
    fn drop(&mut self) {
        let _ = self.obj.tx.lock().unwrap().broadcast(());
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn rate_meter() {
        let mut meter = RateMeter::seconds();
        assert_eq!(meter.tick(1), None);
        assert_eq!(meter.tick(2), None);
        assert_eq!(meter.tick(3), None);
        std::thread::sleep(Duration::from_secs(1));
        assert!(meter
            .tick(0)
            .map(|val| (val - 6.0).abs() <= 1e-3)
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn watched_object() -> Fallible<()> {
        struct MockedState(usize);

        let n_watchers = 1;
        let obj = WatchedObject::new(MockedState(0));

        let watcher_futures = (0..n_watchers)
            .into_iter()
            .map(|_| {
                let mut obj_clone = obj.clone();
                async move {
                    tokio::spawn(async move {
                        obj_clone.watch().await.unwrap();
                        assert_eq!(obj_clone.read().await.0, 0);

                        obj_clone.watch().await.unwrap();
                        assert_eq!(obj_clone.read().await.0, 3);

                        obj_clone.watch().await.unwrap();
                        assert_eq!(obj_clone.read().await.0, 1);

                        obj_clone.watch().await.unwrap();
                        assert_eq!(obj_clone.read().await.0, 4);

                        Fallible::Ok(())
                    })
                    .await??;
                    Fallible::Ok(())
                }
            })
            .collect::<Vec<_>>();

        let sender_future = async move {
            tokio::spawn(async move {
                tokio::time::delay_for(Duration::from_millis(1000)).await;

                obj.write().await.0 = 3;
                tokio::time::delay_for(Duration::from_millis(200)).await;

                obj.write().await.0 = 1;
                tokio::time::delay_for(Duration::from_millis(200)).await;

                obj.write().await.0 = 4;
                tokio::time::delay_for(Duration::from_millis(200)).await;
            })
            .await?;
            Fallible::Ok(())
        };

        futures::try_join!(
            sender_future,
            futures::future::try_join_all(watcher_futures),
        )?;

        Ok(())
    }
}
