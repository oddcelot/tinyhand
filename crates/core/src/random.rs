/// Tiny xorshift: enough randomness for particles without another crate.
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Self {
        Random(seed | 1)
    }

    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Random::new(nanos ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// A number in 0..1.
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    /// `base` give or take `range`.
    pub fn around(&mut self, base: f32, range: f32) -> f32 {
        base + (self.next() * 2.0 - 1.0) * range
    }
}
