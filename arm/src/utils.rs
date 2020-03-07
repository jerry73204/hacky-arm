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
