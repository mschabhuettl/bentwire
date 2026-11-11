//! WMF rasteriser (D2, act-format 4.6) on synthetic metafiles from `agent-fixtures`:
//! hand-computable pixel assertions plus golden images in `tests/golden/wmf_*.png`.
//! Regenerate goldens with `BENTWIRE_BLESS=1 cargo test -p bentwire-convert --test wmf_golden`
//! and look at every changed PNG before committing it.

mod wmf_common;

use agent_fixtures::act::{WmfBuilder, wmf_actor};
use agent_formats::act::{self, Payload, Wmf};
use bentwire_convert::wmf::{self, Canvas, PixelRect, WmfError};

const RED: [u8; 3] = [0xC0, 0x20, 0x20];
const BLUE: [u8; 3] = [0x20, 0x40, 0xC0];
const BLACK: [u8; 3] = [0, 0, 0];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const GREEN: [u8; 3] = [0x20, 0xA0, 0x40];
const T: u8 = 5;

fn palette() -> Vec<[u8; 3]> {
    vec![RED, BLUE, BLACK, WHITE, GREEN, [0xFF, 0x00, 0xFF]]
}

/// Parse a placeable WMF built by the fixture writer (through a one-part actor).
fn wmf(bytes: Vec<u8>) -> Wmf {
    let mut a = wmf_actor();
    a.payloads = vec![agent_fixtures::act::PayloadSpec::Wmf(bytes)];
    a.rest_pose = 0;
    a.animations = vec![(1, vec![vec![(0, 0, 100)]])];
    a.sounds.clear();
    match act::parse(&a.to_bytes()).unwrap().payloads.remove(0) {
        Payload::Wmf(w) => w,
        p => panic!("{p:?}"),
    }
}

fn render(b: WmfBuilder, w: u32, h: u32) -> Canvas {
    let mut c = Canvas::new(w, h, palette(), T);
    let dest = PixelRect {
        left: 0,
        top: 0,
        right: w as i32,
        bottom: h as i32,
    };
    wmf::draw(&mut c, &wmf(b.build()), dest).unwrap();
    c
}

fn count(c: &Canvas, idx: u8) -> usize {
    c.pixels.iter().filter(|&&p| p == idx).count()
}

fn at(c: &Canvas, x: u32, y: u32) -> u8 {
    c.pixels[(y * c.width + x) as usize]
}

fn golden(name: &str, c: &Canvas) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/golden/wmf_{name}.png"));
    if std::env::var_os("BENTWIRE_BLESS").is_some() {
        std::fs::write(&path, wmf_common::encode_png(c)).unwrap();
        return;
    }
    let want = std::fs::read(&path).unwrap_or_else(|_| panic!("missing golden {}", path.display()));
    let (w, h, px, pal) = wmf_common::decode_png(&want);
    assert_eq!((w, h), (c.width, c.height), "{name}: size");
    assert_eq!(pal, c.palette, "{name}: palette");
    assert!(px == c.pixels, "{name}: pixels differ from golden");
}

/// 1:1 mapping on a 64 x 64 canvas, null pen, solid red brush.
fn base() -> WmfBuilder {
    WmfBuilder::new([0, 0, 64, 64])
        .window([0, 0], [64, 64])
        .pen(5, 0, BLACK)
        .brush(0, RED)
        .select(0)
        .select(1)
}

#[test]
fn d2_wmf_axis_aligned_polygon_exact_pixels() {
    // pixel-centre rule: x in 10..30, y in 10..20 -> 20 x 10 pixels
    let c = render(
        base().polygon(&[(10, 10), (30, 10), (30, 20), (10, 20)]),
        64,
        64,
    );
    assert_eq!(count(&c, 0), 200);
    assert_eq!(at(&c, 10, 10), 0);
    assert_eq!(at(&c, 29, 19), 0);
    assert_eq!(at(&c, 30, 19), T);
    assert_eq!(at(&c, 29, 20), T);
}

#[test]
fn d2_wmf_alternate_vs_winding() {
    // two overlapping squares drawn as one self-overlapping polygon pair (PolyPolygon,
    // same orientation): the overlap is a hole under ALTERNATE, filled under WINDING
    let a: &[(i16, i16)] = &[(8, 8), (40, 8), (40, 40), (8, 40)];
    let b: &[(i16, i16)] = &[(24, 24), (56, 24), (56, 56), (24, 56)];
    let alt = render(base().poly_fill_mode(1).poly_polygon(&[a, b]), 64, 64);
    let win = render(base().poly_fill_mode(2).poly_polygon(&[a, b]), 64, 64);
    assert_eq!(at(&alt, 30, 30), T);
    assert_eq!(at(&win, 30, 30), 0);
    assert_eq!(count(&win, 0), 32 * 32 * 2 - 16 * 16);
    assert_eq!(count(&alt, 0), 32 * 32 * 2 - 2 * 16 * 16);
    golden("fill_alternate", &alt);
    golden("fill_winding", &win);
}

#[test]
fn d2_wmf_magic_colour_paints_transparent() {
    // act-format 4.6: COLORREF 0xF5FAF5 means transparent
    let c = render(
        base()
            .polygon(&[(0, 0), (64, 0), (64, 64), (0, 64)])
            .brush(0, [0xF5, 0xFA, 0xF5])
            .select(2)
            .polygon(&[(16, 16), (48, 16), (48, 48), (16, 48)]),
        64,
        64,
    );
    assert_eq!(count(&c, T), 32 * 32);
    assert_eq!(count(&c, 0), 64 * 64 - 32 * 32);
}

#[test]
fn d2_wmf_colours_snap_to_nearest_palette_entry() {
    let c = render(
        base()
            .brush(0, [0xB0, 0x30, 0x28])
            .select(2)
            .polygon(&[(0, 0), (8, 0), (8, 8), (0, 8)]),
        64,
        64,
    );
    assert_eq!(count(&c, 0), 64);
}

#[test]
fn d2_wmf_notxorpen_on_opaque_and_transparent() {
    // W-ROP-1: R2_NOTXORPEN = NOT(dst XOR pen) per channel, snapped; transparent stays
    let c = render(
        base()
            .polygon(&[(0, 0), (32, 0), (32, 64), (0, 64)])
            .rop2(10)
            .brush(0, BLACK)
            .select(2)
            .polygon(&[(16, 16), (48, 16), (48, 48), (16, 48)]),
        64,
        64,
    );
    // NOT(red XOR black) = (3F, DF, DF): nearest is blue (distance 27203; white 38912)
    assert_eq!(at(&c, 20, 20), 1);
    assert_eq!(at(&c, 40, 20), T);
    assert_eq!(at(&c, 5, 5), 0);
}

#[test]
fn d2_wmf_window_records_and_save_restore() {
    // window 0..640 maps 10 logical units to 1 pixel; SaveDC/RestoreDC restores the window
    let c = render(
        WmfBuilder::new([0, 0, 64, 64])
            .window([0, 0], [640, 640])
            .pen(5, 0, BLACK)
            .brush(0, BLUE)
            .select(0)
            .select(1)
            .save_dc()
            .window([0, 0], [64, 64])
            .restore_dc(-1)
            .polygon(&[(100, 100), (200, 100), (200, 200), (100, 200)]),
        64,
        64,
    );
    assert_eq!(count(&c, 1), 100);
    assert_eq!(at(&c, 10, 10), 1);
}

#[test]
fn d2_wmf_null_brush_and_cosmetic_pen_outline() {
    // hollow square outlined with a 1-pixel pen: 4 edges of 20 px, corners once
    let c = render(
        WmfBuilder::new([0, 0, 64, 64])
            .window([0, 0], [64, 64])
            .pen(0, 0, BLACK)
            .brush(1, RED)
            .select(0)
            .select(1)
            .polygon(&[(10, 10), (30, 10), (30, 30), (10, 30)]),
        64,
        64,
    );
    assert_eq!(count(&c, 0), 0);
    assert_eq!(count(&c, 2), 80);
}

#[test]
fn d2_wmf_shapes_golden() {
    let c = render(
        WmfBuilder::new([0, 0, 1000, 1000])
            .window([0, 0], [1000, 1000])
            .pen(0, 40, BLACK)
            .brush(0, RED)
            .select(0)
            .select(1)
            .ellipse([50, 50, 450, 350])
            .pen(6, 60, BLUE)
            .select(2)
            .rect_round([550, 50, 950, 450], [200, 120])
            .pen(0, 0, GREEN)
            .select(3)
            .ellipse([550, 550, 950, 700])
            .pen(0, 30, GREEN)
            .select(4)
            .polyline(&[(50, 950), (250, 600), (450, 950)])
            .pen(5, 0, BLACK)
            .select(5)
            .brush(0, BLUE)
            .select(6)
            .poly_polygon(&[
                &[(550, 750), (950, 750), (750, 980)],
                &[(700, 800), (800, 800), (750, 900)],
            ]),
        96,
        96,
    );
    golden("shapes_1x", &c);
}

#[test]
fn d2_wmf_text_golden() {
    let b = WmfBuilder::new([0, 0, 200, 60])
        .window([0, 0], [200, 60])
        .text_color(BLACK)
        .bk_mode(1)
        .text_align(24)
        .font(-24, 700, "Albertus Medium")
        .select(0)
        .text_out(4, 40, b"Ab*?")
        .bk_mode(2)
        .bk_color(WHITE)
        .font(-28, 400, "Courier New")
        .select(1)
        .text_align(0)
        .text_out(110, 16, b"liv");
    let c = render(b, 100, 30);
    assert!(count(&c, 2) > 40, "glyph pixels");
    golden("text", &c);
}

#[test]
fn d2_wmf_fixture_actor_poses_1x_2x_golden() {
    let f = act::parse(&wmf_actor().to_bytes()).unwrap();
    let mut pal = f.color_table.clone();
    let t = pal.len() as u8;
    pal.push([0xFF, 0x00, 0xFF]);
    for (pose, scale) in [(2u16, 1u32), (2, 2), (3, 1)] {
        let mut c = Canvas::new(192 * scale, 144 * scale, pal.clone(), t);
        wmf::draw_pose(&mut c, &f, pose).unwrap();
        assert!(c.pixels.iter().any(|&p| p != t));
        golden(&format!("actor_pose{pose}_x{scale}"), &c);
    }
}

#[test]
fn d2_wmf_map_rect_truncates_toward_zero() {
    let r = wmf::map_rect(-180, -200, 3000, 2401, (2880, 2160), (192, 144));
    assert_eq!(
        r,
        PixelRect {
            left: -12,
            top: -13,
            right: 200,
            bottom: 160
        }
    );
    let r = wmf::map_rect(-7, 0, 7, 14, (15, 15), (1, 1));
    assert_eq!((r.left, r.right, r.bottom), (0, 0, 0));
}

#[test]
fn d2_wmf_errors_not_panics() {
    let mut c = Canvas::new(8, 8, palette(), T);
    let d = PixelRect {
        left: 0,
        top: 0,
        right: 8,
        bottom: 8,
    };
    // unsupported record
    let w = wmf(base().record(0x0B41, &[0; 10]).build());
    assert!(matches!(
        wmf::draw(&mut c, &w, d),
        Err(WmfError::Unsupported {
            function: 0x0B41,
            ..
        })
    ));
    // polygon claiming more points than present
    let w = wmf(base().record(0x0324, &[50, 1, 1]).build());
    assert!(matches!(
        wmf::draw(&mut c, &w, d),
        Err(WmfError::Malformed { .. })
    ));
    // zero window extent draws nothing
    let w = wmf(base()
        .window([0, 0], [0, 0])
        .polygon(&[(0, 0), (8, 0), (8, 8)])
        .build());
    assert!(wmf::draw(&mut c, &w, d).is_ok());
    assert_eq!(count(&c, T), 64);
}
