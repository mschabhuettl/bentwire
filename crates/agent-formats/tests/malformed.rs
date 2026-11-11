//! Malformed input yields `Err`, never a panic: a deterministic mutation loop (truncation,
//! bit flips, extreme values, insertions) over every fixture kind.

use std::panic::{AssertUnwindSafe, catch_unwind};

use agent_fixtures::common::Compression;
use agent_formats::acf::{Aca, AcaAnimation, Acf, AcfCharacter};
use agent_formats::acs::Acs;
use agent_formats::agent15::OleAcs;
use agent_formats::lz;

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

/// Runs `parse` on `rounds` mutants of `data` (plus every prefix at a stride) and fails with
/// the mutation that panicked.
fn fuzz(name: &str, data: &[u8], rounds: usize, parse: &dyn Fn(&[u8])) {
    let run = |what: String, d: &[u8]| {
        if catch_unwind(AssertUnwindSafe(|| parse(d))).is_err() {
            panic!("{name}: panic on {what}");
        }
    };
    run("original".into(), data);
    let step = (data.len() / 300).max(1);
    for n in (0..data.len()).step_by(step) {
        run(format!("truncation to {n}"), &data[..n]);
    }
    let mut rng = Rng(data.len() as u64 ^ 0x5EED);
    for round in 0..rounds {
        let mut d = data.to_vec();
        let kind = round % 5;
        for _ in 0..1 + rng.below(4) {
            if d.is_empty() {
                break;
            }
            let i = rng.below(d.len());
            match kind {
                0 => d[i] ^= 1 << rng.below(8),
                1 => d[i] = rng.next() as u8,
                2 => {
                    // an extreme little-endian u32 / u16 at a random spot
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
        run(format!("mutation round {round}"), &d);
    }
}

/// Mutations per fixture; `BENTWIRE_FUZZ_ROUNDS` raises it for longer runs.
fn rounds() -> usize {
    std::env::var("BENTWIRE_FUZZ_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500)
}

#[test]
fn acs_malformed_never_panics() {
    let mut s = agent_fixtures::acs::sample();
    s.images[4].picture = None; // include a kind-0 record
    let bytes = s.write();
    fuzz("acs", &bytes, rounds(), &|d| {
        let _ = Acs::parse(d);
    });
    // a small character so that mutations hit structure more often than pixels
    s.width = 8;
    s.height = 6;
    for img in &mut s.images {
        if let Some(p) = &mut img.picture {
            *p = agent_fixtures::common::Picture::pattern(5, 4, p.width as u32, s.transparent);
        }
    }
    s.palette.truncate(4);
    fuzz("acs small", &s.write(), rounds() * 3, &|d| {
        let _ = Acs::parse(d);
    });
}

#[test]
fn acf_and_aca_malformed_never_panic() {
    for v in agent_formats::acf::VERSIONS {
        let mut s = agent_fixtures::acf::sample(v);
        s.palette.truncate(8);
        let acf = s.write_acf();
        fuzz("acf file", &acf, rounds() / 3, &|d| {
            let _ = Acf::parse(d);
        });
        let body = s.body();
        fuzz("acf body", &body, rounds(), &|d| {
            let _ = AcfCharacter::parse_body(d);
        });
        let c = Acf::parse(&acf).unwrap().character;
        for i in 0..s.animations.len() {
            fuzz("aca body", &s.aca_body(i), rounds(), &|d| {
                let _ = AcaAnimation::parse_body(d, &c);
            });
            s.animations[i].compression = Compression::Raw;
            fuzz("aca raw file", &s.write_aca(i), rounds() / 3, &|d| {
                let _ = Aca::parse(d, &c);
            });
            s.animations[i].compression = Compression::Lz;
            fuzz("aca compressed file", &s.write_aca(i), rounds() / 3, &|d| {
                let _ = Aca::parse(d, &c);
            });
        }
    }
}

#[test]
fn ole_malformed_never_panics() {
    for v in [0x1_001F, 0x1_001E, 0x1_001C] {
        let bytes = agent_fixtures::agent15::write_ole(&agent_fixtures::agent15::sample(v));
        fuzz("ole", &bytes, rounds(), &|d| {
            let _ = OleAcs::parse(d);
        });
        // mutations restricted to the header and directory sectors, where the container
        // structure lives
        let mut rng = Rng(v as u64);
        let dir_start = u32::from_le_bytes(bytes[0x30..0x34].try_into().unwrap()) as usize;
        let ranges = [
            0..512,
            (dir_start + 1) * 512..(dir_start + 2) * 512,
            bytes.len() - 512..bytes.len(),
        ];
        for round in 0..rounds() {
            let mut d = bytes.clone();
            let r = &ranges[round % ranges.len()];
            for _ in 0..1 + rng.below(3) {
                let i = r.start + rng.below(r.end - r.start);
                d[i] = if round % 2 == 0 {
                    rng.next() as u8
                } else {
                    0xFF
                };
            }
            if catch_unwind(AssertUnwindSafe(|| OleAcs::parse(&d))).is_err() {
                panic!("ole {v:#x}: panic in structured round {round}");
            }
        }
    }
}

#[test]
fn lz_malformed_never_panics() {
    let data: Vec<u8> = (0..3000u32).map(|i| (i * i % 7) as u8).collect();
    let z = agent_fixtures::common::lz_compress(&data, true);
    assert_eq!(lz::decompress_exact(&z, data.len()).unwrap(), data);
    fuzz("lz", &z, rounds() * 2, &|d| {
        let _ = lz::decompress(d, 4000);
        let _ = lz::decompress(d, 0);
    });
}

/// A-FMT-1: list entries that all point at the same large block cannot multiply memory.
/// 40,000 audio entries each covering the whole file would copy more than 1 GiB.
#[test]
fn aliased_entries_hit_the_total_budget() {
    let mut d = agent_fixtures::acs::sample().write();
    let len = d.len() as u32;
    let n = (agent_formats::common::MAX_TOTAL / len as usize + 1) as u32;
    let list = d.len() as u32;
    d.extend_from_slice(&n.to_le_bytes());
    for _ in 0..n {
        for v in [0, len, 0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    let size = d.len() as u32 - list;
    d[0x1C..0x20].copy_from_slice(&list.to_le_bytes());
    d[0x20..0x24].copy_from_slice(&size.to_le_bytes());
    let e = Acs::parse(&d).unwrap_err();
    assert!(e.to_string().contains("in total"), "{e}");
}
