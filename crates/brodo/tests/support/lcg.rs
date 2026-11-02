//! A linear congruential sequence, Knuth's MMIX constants, its high bits taken: values a
//! test draws from a seed, the same on every machine.

pub struct Lcg(pub u64);

impl Lcg {
    /// The next value, 31 bits.
    pub fn next(&mut self) -> u32 {
        self.0 =
            self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        u32::try_from(self.0 >> 33).unwrap_or(u32::MAX)
    }
}
