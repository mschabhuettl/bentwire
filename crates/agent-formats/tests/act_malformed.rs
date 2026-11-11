//! Malformed ACT input yields `Err`, never a panic (act-format 4): deterministic mutations
//! (truncation, bit flips, extreme values, insertions, deletions) of every ACT fixture.
//! `BENTWIRE_FUZZ_ROUNDS` raises the number of mutants per fixture (default 1500).

use std::panic::{AssertUnwindSafe, catch_unwind};

use agent_fixtures::act::{bitmap_actor, bob_actor, wmf_actor};
use agent_formats::act;

/// splitmix64
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn mutate(rng: &mut Rng, data: &[u8], kind: usize) -> Vec<u8> {
    let mut d = data.to_vec();
    for _ in 0..1 + rng.below(4) {
        if d.is_empty() {
            break;
        }
        let i = rng.below(d.len());
        match kind % 5 {
            0 => d[i] ^= 1 << rng.below(8),
            1 => d[i] = rng.next() as u8,
            2 => {
                let v: [u8; 4] =
                    [[0xFF; 4], [0; 4], [0xFF, 0xFF, 0, 0], [0, 0, 0, 0x80]][rng.below(4)];
                for (k, b) in v.iter().enumerate() {
                    if i + k < d.len() {
                        d[i + k] = *b;
                    }
                }
            }
            3 => {
                let n = 1 + rng.below(8);
                let ins: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
                d.splice(i..i, ins);
            }
            _ => {
                let n = rng.below(16).min(d.len() - i);
                d.drain(i..i + n);
            }
        }
    }
    d
}

fn rounds() -> usize {
    std::env::var("BENTWIRE_FUZZ_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500)
}

fn fuzz(name: &str, data: &[u8]) {
    let run = |what: String, d: &[u8]| {
        if catch_unwind(AssertUnwindSafe(|| {
            let _ = act::parse(d);
        }))
        .is_err()
        {
            panic!("{name}: panic on {what}");
        }
    };
    run("original".into(), data);
    for n in (0..data.len()).step_by((data.len() / 300).max(1)) {
        run(format!("truncation to {n}"), &data[..n]);
    }
    let mut rng = Rng(data.len() as u64 ^ 0xAC7);
    let mut errs = 0;
    for round in 0..rounds() {
        let d = mutate(&mut rng, data, round);
        run(format!("mutation round {round}"), &d);
        errs += usize::from(act::parse(&d).is_err());
    }
    assert!(errs > 0, "{name}: no mutant was rejected");
}

#[test]
fn act_malformed_bitmap_actor_never_panics() {
    fuzz("bitmap actor", &bitmap_actor().to_bytes());
}

#[test]
fn act_malformed_wmf_actor_never_panics() {
    fuzz("wmf actor", &wmf_actor().to_bytes());
}

#[test]
fn act_malformed_bob_actor_never_panics() {
    fuzz("bob actor", &bob_actor().to_bytes());
}

#[test]
fn act_malformed_lz_streams_never_panic() {
    let data: Vec<u8> = (0..5000u32).map(|i| (i * i / 7) as u8).collect();
    let c = agent_fixtures::common::lz_compress(&data, true);
    let mut rng = Rng(7);
    for round in 0..rounds() {
        let d = mutate(&mut rng, &c, round);
        let size = [data.len(), 0, 1 << 20, usize::MAX][round % 4];
        if catch_unwind(|| act::lz::decompress(&d, size)).is_err() {
            panic!("lz: panic on round {round}");
        }
    }
}

#[test]
fn act_malformed_mso97_palette_never_panics() {
    // a PE-shaped buffer mutated; must fail cleanly (no Microsoft data involved)
    let mut pe = vec![0u8; 0x400];
    pe[..2].copy_from_slice(b"MZ");
    pe[0x3C] = 0x80;
    pe[0x80..0x84].copy_from_slice(b"PE\0\0");
    pe[0x86] = 1;
    pe[0x94] = 0xE0;
    pe[0x98..0x9A].copy_from_slice(&0x10Bu16.to_le_bytes());
    let mut rng = Rng(9);
    for round in 0..rounds() {
        let d = mutate(&mut rng, &pe, round);
        if catch_unwind(|| act::office_palette_from_mso97(&d)).is_err() {
            panic!("palette: panic on round {round}");
        }
    }
}
