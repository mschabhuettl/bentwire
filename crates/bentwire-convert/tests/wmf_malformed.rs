//! Malformed WMF input gives `Err` (or draws within the canvas), never a panic (D2):
//! mutated vector actors are parsed and every pose is rasterised; extreme destination
//! rectangles are drawn. `BENTWIRE_FUZZ_ROUNDS` raises the number of mutants (default 1500).

use std::panic::{AssertUnwindSafe, catch_unwind};

use agent_fixtures::act::{PayloadSpec, WmfBuilder, wmf_actor};
use agent_formats::act::{self, Payload};
use bentwire_convert::wmf::{self, Canvas, PixelRect};

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

fn rounds() -> usize {
    std::env::var("BENTWIRE_FUZZ_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500)
}

/// A vector actor whose parts use all 24 record types.
fn rich_actor() -> Vec<u8> {
    let part = WmfBuilder::new([0, 0, 100, 100])
        .window([0, 0], [100, 100])
        .comment(b"x")
        .save_dc()
        .pen(6, 8, [1, 2, 3])
        .brush(0, [9, 9, 9])
        .font(-20, 700, "Albertus Medium")
        .select(0)
        .select(1)
        .select(2)
        .poly_fill_mode(2)
        .rop2(10)
        .bk_mode(2)
        .bk_color([255, 255, 255])
        .text_color([0, 0, 0])
        .text_align(24)
        .polygon(&[(0, 0), (90, 10), (50, 90)])
        .polyline(&[(0, 0), (100, 100), (0, 100)])
        .poly_polygon(&[
            &[(10, 10), (40, 10), (40, 40)],
            &[(50, 50), (90, 50), (90, 90)],
        ])
        .ellipse([10, 10, 60, 40])
        .rect_round([20, 20, 80, 80], [10, 10])
        .text_out(10, 90, b"Hi")
        .delete(0)
        .restore_dc(-1)
        .build();
    let mut a = wmf_actor();
    a.payloads.insert(0, PayloadSpec::Wmf(part));
    a.payloads.push(PayloadSpec::Composite(vec![
        (0, -100, -100, 3000, 2300),
        (1, 0, 0, 2880, 2160),
    ]));
    a.rest_pose = 3;
    a.animations = vec![(1, vec![vec![(0, 3, 100), (0, 4, 100), (0, 5, 100)]])];
    a.to_bytes()
}

fn draw_all(bytes: &[u8]) {
    let Ok(f) = act::parse(bytes) else { return };
    let mut pal = wmf::collect_colors(&f);
    pal.truncate(255);
    let t = pal.len() as u8;
    pal.push([0xFF, 0, 0xFF]);
    for pose in 0..f.payloads.len() as u16 {
        let mut c = Canvas::new(48, 36, pal.clone(), t);
        let _ = wmf::draw_pose(&mut c, &f, pose);
        assert!(c.pixels.iter().all(|&p| usize::from(p) < pal.len()));
    }
}

#[test]
fn d2_wmf_malformed_actor_never_panics() {
    for (name, data) in [("rich", rich_actor()), ("fixture", wmf_actor().to_bytes())] {
        let mut rng = Rng(data.len() as u64);
        for round in 0..rounds() {
            let mut d = data.clone();
            for _ in 0..1 + rng.below(4) {
                let i = rng.below(d.len());
                match round % 3 {
                    0 => d[i] ^= 1 << rng.below(8),
                    1 => d[i] = rng.next() as u8,
                    _ => {
                        let v: [u8; 2] =
                            [[0xFF, 0x7F], [0, 0x80], [0xFF, 0xFF], [0, 0]][rng.below(4)];
                        d[i] = v[0];
                        if i + 1 < d.len() {
                            d[i + 1] = v[1];
                        }
                    }
                }
            }
            if catch_unwind(AssertUnwindSafe(|| draw_all(&d))).is_err() {
                panic!("{name}: panic on mutation round {round}");
            }
        }
    }
}

#[test]
fn d2_wmf_extreme_destination_rectangles_never_panic() {
    let f = act::parse(&rich_actor()).unwrap();
    let Payload::Wmf(w) = &f.payloads[0] else {
        panic!()
    };
    let pal = vec![[0, 0, 0], [255, 255, 255], [255, 0, 255]];
    let mut rng = Rng(42);
    let ext = [i32::MIN, -100_000, -1, 0, 1, 47, 100_000, i32::MAX];
    for round in 0..rounds() {
        let mut v = || ext[rng.below(ext.len())];
        let d = PixelRect {
            left: v(),
            top: v(),
            right: v(),
            bottom: v(),
        };
        let mut c = Canvas::new(32, 24, pal.clone(), 2);
        if catch_unwind(AssertUnwindSafe(|| {
            let _ = wmf::draw(&mut c, w, d);
        }))
        .is_err()
        {
            panic!("panic on dest {d:?} (round {round})");
        }
    }
}

#[test]
fn d2_wmf_long_wide_pen_zigzag_is_bounded() {
    // review finding: 16,000-point polyline with a 2 px pen took 8 s before per-row spans
    let pts: Vec<(i16, i16)> = (0..16_000)
        .map(|i| ((i % 2) * 380, (i % 288) as i16))
        .collect();
    let part = WmfBuilder::new([0, 0, 384, 288])
        .window([0, 0], [384, 288])
        .pen(0, 2, [0, 0, 0])
        .select(0)
        .polyline(&pts)
        .build();
    let mut a = wmf_actor();
    a.payloads = vec![PayloadSpec::Wmf(part)];
    a.rest_pose = 0;
    a.animations = vec![(1, vec![vec![(0, 0, 100)]])];
    let f = act::parse(&a.to_bytes()).unwrap();
    let Payload::Wmf(w) = &f.payloads[0] else {
        panic!()
    };
    let mut c = Canvas::new(384, 288, vec![[0, 0, 0], [255, 0, 255]], 1);
    let t = std::time::Instant::now();
    wmf::draw(
        &mut c,
        w,
        PixelRect {
            left: 0,
            top: 0,
            right: 384,
            bottom: 288,
        },
    )
    .unwrap();
    let s = t.elapsed().as_secs_f64();
    assert!(s < 20.0, "{s} s");
    assert!(c.pixels.iter().any(|&p| p == 0));
}
