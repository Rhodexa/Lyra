//! Small deterministic randomness. Same seed, same performance, every time.

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    /// xorshift64*
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// 0..1
    pub fn uniform(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// -1..1
    pub fn bipolar(&mut self) -> f32 {
        self.uniform() * 2.0 - 1.0
    }
}

/// A repeatable -1..1 value for "thing number `i`" under `seed`.
pub fn hash_bipolar(seed: u64, i: usize) -> f32 {
    let mut r = Rng::new(seed ^ (i as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93));
    r.next_u64();
    r.bipolar()
}

/// Smooth random wander in -1..1, changing direction about `rate` times a second.
#[derive(Clone, Debug)]
pub struct Wander {
    rng: Rng,
    a: f32,
    b: f32,
    phase: f32,
    rate: f32,
}

impl Wander {
    pub fn new(seed: u64, rate: f32) -> Self {
        let mut rng = Rng::new(seed);
        let a = rng.bipolar();
        let b = rng.bipolar();
        Self { rng, a, b, phase: 0.0, rate }
    }
    pub fn next(&mut self, dt: f32) -> f32 {
        self.phase += self.rate * dt;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.a = self.b;
            self.b = self.rng.bipolar();
        }
        let t = (1.0 - (std::f32::consts::PI * self.phase).cos()) * 0.5;
        self.a + (self.b - self.a) * t
    }
}
