//! Random sources (B-CORE-2).

use std::collections::VecDeque;

use crate::api::Draws;

/// splitmix64, `draw(s) = next() % s` (A-TEST-2; mirrors the oracle driver).
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Draws for SplitMix64 {
    fn draw(&mut self, n: u32) -> u32 {
        (self.next_u64() % u64::from(n.max(1))) as u32
    }
}

/// Scripted draws for tests and `Test1.SetDraws`: each value is taken modulo `n`; when
/// the script is exhausted every draw returns 0.
#[derive(Clone, Debug, Default)]
pub struct ScriptedDraws {
    values: VecDeque<u32>,
    /// Number of draws taken so far.
    pub taken: usize,
}

impl ScriptedDraws {
    pub fn new(values: impl IntoIterator<Item = u32>) -> Self {
        ScriptedDraws {
            values: values.into_iter().collect(),
            taken: 0,
        }
    }
}

impl Draws for ScriptedDraws {
    fn draw(&mut self, n: u32) -> u32 {
        self.taken += 1;
        self.values.pop_front().unwrap_or(0) % n.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_core_2_draw_sources() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        let va: Vec<u32> = (0..20).map(|_| a.draw(100)).collect();
        let vb: Vec<u32> = (0..20).map(|_| b.draw(100)).collect();
        assert_eq!(va, vb);
        assert!(va.iter().all(|&v| v < 100));
        // splitmix64 reference value for seed 0.
        assert_eq!(SplitMix64::new(0).next_u64(), 0xE220_A839_7B1D_CDAF);
        let mut s = ScriptedDraws::new([5, 250, 7]);
        assert_eq!(
            (s.draw(10), s.draw(100), s.draw(3), s.draw(9)),
            (5, 50, 1, 0)
        );
        assert_eq!(s.taken, 4);
    }
}
