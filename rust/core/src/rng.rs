//! A small deterministic PRNG.
//!
//! The Swift build uses `SystemRandomNumberGenerator`, so every run of
//! `--simtest` produces slightly different numbers and the per-neuron baselines
//! (`Sim.swift:163`, `Float.random(in: 0.010...0.070)` for `other` neurons) are
//! redrawn on every launch. That is a deliberate deviation in the port: the
//! suites are the ground truth, and a ground truth that changes run to run is a
//! weaker one. Seeding explicitly makes both suites reproducible.
//!
//! The seed is part of the creature's configuration, not a global — a
//! connectome plus a seed fully determines a run.

/// PCG-XSH-RR 64/32. Small, fast, and good enough for synaptic noise.
#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    pub const DEFAULT_SEED: u64 = 0x5EED_F1_1E5;

    pub fn new(seed: u64) -> Self {
        let mut r = Pcg32 {
            state: 0,
            inc: (seed << 1) | 1,
        };
        r.next_u32();
        r.state = r.state.wrapping_add(seed);
        r.next_u32();
        r
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in [0, 1). Matches the use of `Float.random(in: 0...1)`.
    #[inline]
    pub fn f32(&mut self) -> f32 {
        // 24 bits of mantissa is exactly what an f32 can represent.
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [lo, hi).
    #[inline]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Uniform integer in [lo, hi], inclusive — matches `Int.random(in: a...b)`.
    ///
    /// An empty range returns `lo` rather than dividing by zero. Callers derive
    /// `hi` from collection lengths (`len - 1`), so an empty collection would
    /// otherwise panic in release with a modulo-by-zero — a latent crash that a
    /// `debug_assert` alone does not catch.
    #[inline]
    pub fn int_range(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u32() as u64 % span) as i64
    }
}

impl Default for Pcg32 {
    fn default() -> Self {
        Pcg32::new(Self::DEFAULT_SEED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f32_is_in_unit_interval() {
        let mut r = Pcg32::new(1);
        for _ in 0..100_000 {
            let v = r.f32();
            assert!((0.0..1.0).contains(&v), "{v} out of range");
        }
    }

    #[test]
    fn same_seed_same_stream() {
        let a: Vec<u32> = (0..64).scan(Pcg32::new(42), |r, _| Some(r.next_u32())).collect();
        let b: Vec<u32> = (0..64).scan(Pcg32::new(42), |r, _| Some(r.next_u32())).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_diverge() {
        let a: Vec<u32> = (0..16).scan(Pcg32::new(1), |r, _| Some(r.next_u32())).collect();
        let b: Vec<u32> = (0..16).scan(Pcg32::new(2), |r, _| Some(r.next_u32())).collect();
        assert_ne!(a, b);
    }

    #[test]
    fn mean_is_roughly_half() {
        let mut r = Pcg32::new(7);
        let n = 200_000;
        let mean: f64 = (0..n).map(|_| r.f32() as f64).sum::<f64>() / n as f64;
        assert!((mean - 0.5).abs() < 0.01, "mean {mean}");
    }

    /// Empty ranges arise from `len - 1` on an empty collection and must not
    /// panic in release.
    #[test]
    fn an_empty_int_range_returns_its_bound_rather_than_dividing_by_zero() {
        let mut r = Pcg32::new(4);
        assert_eq!(r.int_range(0, -1), 0);
        assert_eq!(r.int_range(7, 7), 7);
        assert_eq!(r.int_range(3, 1), 3);
    }

    #[test]
    fn int_range_is_inclusive_and_covers_ends() {
        let mut r = Pcg32::new(9);
        let (mut lo_hit, mut hi_hit) = (false, false);
        for _ in 0..10_000 {
            let v = r.int_range(3, 7);
            assert!((3..=7).contains(&v));
            lo_hit |= v == 3;
            hi_hit |= v == 7;
        }
        assert!(lo_hit && hi_hit, "range ends never drawn");
    }
}
