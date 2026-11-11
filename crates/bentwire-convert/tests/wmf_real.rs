//! Real-data test (A-TEST-1, A-TEST-2, D2): rasterise every pose of every vector actor in
//! the sample set at 1x and 2x and check structural rules. Pictures go to a temp dir only
//! (they are Microsoft-derived); set BENTWIRE_WMF_OUT to keep them in a directory of your
//! choice outside the repo.

mod wmf_common;

use std::collections::BTreeSet;
use std::path::PathBuf;

use agent_formats::act::{self, CMD_FRAME, NONE, Payload};
use bentwire_convert::wmf::{self, Canvas};

fn samples() -> Vec<PathBuf> {
    let r = match std::env::var_os("BENTWIRE_REFERENCE") {
        Some(p) => PathBuf::from(p),
        None => panic!("NOT RUN: set BENTWIRE_REFERENCE=$BENTWIRE_REFERENCE"),
    };
    let base = r.join("act/downloads");
    let mut out = Vec::new();
    for (dir, all) in [("source-files", true), ("source-files", false)] {
        let mut v: Vec<PathBuf> = std::fs::read_dir(base.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        v.sort();
        out.extend(v.into_iter().filter(|p| {
            let n = p.to_string_lossy().to_uppercase();
            all || n.ends_with(".ACT") || n.ends_with(".ACP")
        }));
    }
    out
}

/// Palette for a vector actor: its colour table (or the colours its metafiles use), plus
/// one transparent slot at the end (D2, character-format 12.5).
pub fn vector_palette(f: &act::ActFile) -> (Vec<[u8; 3]>, u8) {
    let mut pal = if f.color_table.is_empty() {
        wmf::collect_colors(f)
    } else {
        f.color_table.clone()
    };
    pal.dedup();
    let t = pal.len() as u8;
    pal.push([0xFF, 0x00, 0xFF]);
    (pal, t)
}

#[test]
#[ignore = "real data: needs BENTWIRE_REFERENCE"]
fn wmf_real_every_vector_pose_rasterises() {
    let keep = std::env::var_os("BENTWIRE_WMF_OUT").map(PathBuf::from);
    let mut actors = 0;
    let mut poses_total = 0;
    let mut problems = Vec::new();
    let mut outside_max = 0usize;
    for path in samples() {
        let f = act::parse(&std::fs::read(&path).unwrap()).unwrap();
        if !f.payloads.iter().any(|p| matches!(p, Payload::Wmf(_))) {
            continue;
        }
        actors += 1;
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let (pal, t) = vector_palette(&f);
        assert!(pal.len() <= 256, "{name}: {} colours", pal.len());
        let mut poses: BTreeSet<u16> = f
            .animations
            .iter()
            .flat_map(|g| &g.variants)
            .flat_map(|v| &v.commands)
            .filter(|c| c.kind == CMD_FRAME && c.a != NONE)
            .map(|c| c.a)
            .collect();
        poses.insert(f.header.rest_pose);
        poses.extend(f.transitions.iter().flat_map(|t| [t.from_pose, t.to_pose]));
        let (w, h) = (f.header.default_px_w as u32, f.header.default_px_h as u32);
        for &pose in &poses {
            poses_total += 1;
            for scale in [1u32, 2] {
                let mut c = Canvas::new(w * scale, h * scale, pal.clone(), t);
                if let Err(e) = wmf::draw_pose(&mut c, &f, pose) {
                    problems.push(format!("{name} pose {pose} x{scale}: {e}"));
                    continue;
                }
                let painted = c.pixels.iter().filter(|&&p| p != t).count();
                if painted == 0 && draws_something(&f, pose) {
                    problems.push(format!("{name} pose {pose} x{scale}: empty"));
                }
                if scale == 1 {
                    outside_max = outside_max.max(outside_pixels(&f, pose, &pal, t));
                }
                if c.pixels.iter().any(|&p| usize::from(p) >= pal.len()) {
                    problems.push(format!(
                        "{name} pose {pose} x{scale}: index outside palette"
                    ));
                }
                if let Some(dir) = &keep {
                    let d = dir.join(name.replace(['(', ')'], "_"));
                    std::fs::create_dir_all(&d).unwrap();
                    std::fs::write(
                        d.join(format!("{pose}_x{scale}.png")),
                        wmf_common::encode_png(&c),
                    )
                    .unwrap();
                }
            }
        }
    }
    println!("max pixels a part paints outside its rectangle plus pen (1x): {outside_max}");
    println!(
        "{actors} vector actors, {poses_total} poses, {} problems",
        problems.len()
    );
    assert!(actors > 0);
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(outside_max, 0, "a part paints outside its rectangle");
}

const DRAWING: [u16; 6] = [0x0324, 0x0325, 0x0538, 0x0418, 0x061C, 0x0521];

fn children(f: &act::ActFile, pose: u16) -> Vec<(u16, [i16; 4])> {
    let (lw, lh) = (f.header.logical_w, f.header.logical_h);
    match &f.payloads[usize::from(pose)] {
        Payload::Composite(cs) => cs
            .iter()
            .map(|c| (c.id, [c.left, c.top, c.right, c.bottom]))
            .collect(),
        _ => vec![(pose, [0, 0, lw, lh])],
    }
}

/// A pose must show something iff one of its metafiles has a drawing record.
fn draws_something(f: &act::ActFile, pose: u16) -> bool {
    children(f, pose)
        .iter()
        .any(|(id, _)| match f.payloads.get(usize::from(*id)) {
            Some(Payload::Wmf(w)) => w.records.iter().any(|r| DRAWING.contains(&r.function)),
            _ => false,
        })
}

/// True if a polygon or polyline point lies outside the metafile's window (bbox when it
/// has no window records). Such parts legitimately paint outside their rectangle.
fn exceeds_window(m: &act::Wmf) -> bool {
    let mut org = (i32::from(m.bbox[0]), i32::from(m.bbox[1]));
    let mut ext = (i32::from(m.bbox[2]) - org.0, i32::from(m.bbox[3]) - org.1);
    for r in &m.records {
        match r.function {
            0x020B => org = (i32::from(r.int(1).unwrap()), i32::from(r.int(0).unwrap())),
            0x020C => ext = (i32::from(r.int(1).unwrap()), i32::from(r.int(0).unwrap())),
            0x0324 | 0x0325 => {
                let n = usize::from(r.word(0).unwrap());
                for k in 0..n {
                    let (x, y) = (
                        i32::from(r.int(1 + 2 * k).unwrap()),
                        i32::from(r.int(2 + 2 * k).unwrap()),
                    );
                    let inx = (x - org.0) * ext.0.signum() >= 0 && (x - org.0).abs() <= ext.0.abs();
                    let iny = (y - org.1) * ext.1.signum() >= 0 && (y - org.1).abs() <= ext.1.abs();
                    if !inx || !iny {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Pixels a part paints outside its destination rectangle grown by half its widest pen
/// plus one pixel (pens are centred on the outline and GDI does not clip to the viewport),
/// largest over the parts. Parts with TextOut are exempt (font metrics differ, W-TEXT-1).
fn outside_pixels(f: &act::ActFile, pose: u16, pal: &[[u8; 3]], t: u8) -> usize {
    let (w, h) = (f.header.default_px_w as u32, f.header.default_px_h as u32);
    let logical = (f.header.logical_w, f.header.logical_h);
    let mut worst = 0;
    for (id, r) in children(f, pose) {
        let Some(Payload::Wmf(m)) = f.payloads.get(usize::from(id)) else {
            continue;
        };
        if m.records.iter().any(|r| r.function == 0x0521) || exceeds_window(m) {
            continue;
        }
        let d = wmf::map_rect(r[0], r[1], r[2], r[3], logical, (w, h));
        let wext = m
            .records
            .iter()
            .rev()
            .find(|r| r.function == 0x020C)
            .and_then(|r| r.int(1))
            .map_or(i32::from(m.bbox[2]) - i32::from(m.bbox[0]), i32::from);
        let pen = m
            .records
            .iter()
            .filter(|r| r.function == 0x02FA)
            .filter_map(|r| r.int(1))
            .max()
            .unwrap_or(0);
        let sx = f64::from(d.right - d.left) / f64::from(wext.abs().max(1));
        let g = (f64::from(pen) * sx / 2.0).ceil() as i32 + 1;
        let mut c = Canvas::new(w, h, pal.to_vec(), t);
        wmf::draw(&mut c, m, d).unwrap();
        let n = (0..h as i32)
            .flat_map(|y| (0..w as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| c.pixels[(y as u32 * w + x as u32) as usize] != t)
            .filter(|&(x, y)| {
                x < d.left - g || x >= d.right + g || y < d.top - g || y >= d.bottom + g
            })
            .count();
        worst = worst.max(n);
    }
    worst
}
