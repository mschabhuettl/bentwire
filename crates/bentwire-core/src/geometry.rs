//! Directions, MoveTo slide and output geometry (R-DIR-1, R-MOVE-1/2, R-GEST-1, B-WL-2/4).

use crate::api::{OutputInfo, Rect};

/// R-DIR-1 result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    ScreenLeft,
    Up,
    ScreenRight,
    Down,
}

/// R-DIR-1: direction of a point relative to a rectangle (truncation toward zero).
pub fn direction(r: Rect, px: i32, py: i32) -> Dir {
    let (l, t, rr, b) = (
        i64::from(r.x),
        i64::from(r.y),
        i64::from(r.x) + i64::from(r.w),
        i64::from(r.y) + i64::from(r.h),
    );
    let (px, py) = (i64::from(px), i64::from(py));
    if rr == l {
        return if px >= l {
            Dir::ScreenRight
        } else {
            Dir::ScreenLeft
        };
    }
    if b == t {
        return if py >= t { Dir::Down } else { Dir::Up };
    }
    let m = (b - t) as f64 / (rr - l) as f64;
    let a = (b as f64 + m * (px - rr) as f64).trunc() as i64;
    let bb = (t as f64 + m * (rr - px) as f64).trunc() as i64;
    match (py > a, py > bb) {
        (true, true) => Dir::Down,
        (true, false) => Dir::ScreenLeft,
        (false, true) => Dir::ScreenRight,
        (false, false) => Dir::Up,
    }
}

/// R-MOVE-1: Moving state for a MoveTo of the frame `r` to top-left `(x, y)`.
pub fn moving_state(r: Rect, x: i32, y: i32) -> Option<&'static str> {
    if (x, y) == (r.x, r.y) {
        return None;
    }
    let d = direction(r, x, y);
    Some(if r.contains(x, y) {
        if d == Dir::ScreenLeft {
            "MovingDown"
        } else {
            "MovingLeft"
        }
    } else {
        match d {
            Dir::ScreenLeft => "MovingRight",
            Dir::Up => "MovingUp",
            Dir::ScreenRight => "MovingLeft",
            Dir::Down => "MovingDown",
        }
    })
}

/// R-GEST-1: Gesturing state for a point.
pub fn gesture_state(r: Rect, x: i32, y: i32) -> &'static str {
    match direction(r, x, y) {
        Dir::ScreenLeft => "GesturingRight",
        Dir::Up => "GesturingUp",
        Dir::ScreenRight => "GesturingLeft",
        Dir::Down => "GesturingDown",
    }
}

/// R-MOVE-2 / B-MOVE-1 slide state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    pub start: (i32, i32),
    pub target: (i32, i32),
    pub period: u64,
    step: i64,
    progressed: i64,
}

impl Slide {
    pub fn new(start: (i32, i32), target: (i32, i32), speed: u32) -> Self {
        let (dx, dy) = (
            (i64::from(target.0) - i64::from(start.0)).abs(),
            (i64::from(target.1) - i64::from(start.1)).abs(),
        );
        let major = dx.max(dy);
        Slide {
            start,
            target,
            period: u64::from(speed / 25).max(10),
            step: (major / 25).max(25),
            progressed: 0,
        }
    }

    pub fn done(&self) -> bool {
        self.position() == self.target
    }

    /// Advances one tick and returns the new position.
    pub fn step(&mut self) -> (i32, i32) {
        self.progressed += self.step;
        self.position()
    }

    pub fn position(&self) -> (i32, i32) {
        let dx = i64::from(self.target.0) - i64::from(self.start.0);
        let dy = i64::from(self.target.1) - i64::from(self.start.1);
        let major = dx.abs().max(dy.abs());
        if major == 0 {
            return self.target;
        }
        let p = self.progressed.min(major);
        // i128: the product of two 33-bit distances does not fit in i64.
        let lerp = |d: i64| (i128::from(d) * i128::from(p) / i128::from(major)) as i64;
        let x = i64::from(self.start.0) + lerp(dx);
        let y = i64::from(self.start.1) + lerp(dy);
        (x as i32, y as i32)
    }
}

/// Frame centre in i64 (positions are not clamped, B-WL-4, so i32 sums can overflow).
fn centre(f: Rect) -> (i64, i64) {
    (
        i64::from(f.x) + i64::from(f.w) / 2,
        i64::from(f.y) + i64::from(f.h) / 2,
    )
}

/// Half-open containment of an i64 point.
fn holds(r: Rect, x: i64, y: i64) -> bool {
    let (l, t) = (i64::from(r.x), i64::from(r.y));
    x >= l && x < l + i64::from(r.w) && y >= t && y < t + i64::from(r.h)
}

/// B-WL-2: the output containing the frame centre, else the nearest one.
pub fn output_for(frame: Rect, outputs: &[OutputInfo]) -> Option<&OutputInfo> {
    let (cx, cy) = centre(frame);
    outputs.iter().find(|o| holds(o.rect, cx, cy)).or_else(|| {
        outputs.iter().min_by_key(|o| {
            let (l, t) = (i64::from(o.rect.x), i64::from(o.rect.y));
            let (r, b) = (l + i64::from(o.rect.w) - 1, t + i64::from(o.rect.h) - 1);
            let ddx = (l - cx).max(0).max(cx - r);
            let ddy = (t - cy).max(0).max(cy - b);
            ddx.saturating_mul(ddx)
                .saturating_add(ddy.saturating_mul(ddy))
        })
    })
}

/// B-WL-4: pull the frame inside `m`, right and bottom edges first, then left and top.
pub fn pull_inside(frame: Rect, m: Rect) -> (i32, i32) {
    let (mut x, mut y) = (i64::from(frame.x), i64::from(frame.y));
    let (w, h) = (i64::from(frame.w), i64::from(frame.h));
    let (ml, mt) = (i64::from(m.x), i64::from(m.y));
    let (mr, mb) = (ml + i64::from(m.w), mt + i64::from(m.h));
    if x + w > mr {
        x = mr - w;
    }
    if y + h > mb {
        y = mb - h;
    }
    if x < ml {
        x = ml;
    }
    if y < mt {
        y = mt;
    }
    (x as i32, y as i32)
}

/// B-WL-4 output change: the output holding the centre, else the first output.
pub fn pull_on_output_change(frame: Rect, outputs: &[OutputInfo]) -> Option<(i32, i32)> {
    let (cx, cy) = centre(frame);
    let m = outputs
        .iter()
        .find(|o| holds(o.rect, cx, cy))
        .or(outputs.first())?;
    Some(pull_inside(frame, m.rect))
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rect = Rect::new(100, 100, 128, 128);

    #[test]
    fn r_dir_1_vectors() {
        use Dir::*;
        let v = [
            ((0, 164), ScreenLeft),
            ((400, 164), ScreenRight),
            ((164, 0), Up),
            ((164, 400), Down),
            ((101, 164), ScreenLeft),
            ((220, 110), ScreenRight),
            ((164, 227), Down),
            ((164, 101), Up),
        ];
        for ((x, y), want) in v {
            assert_eq!(direction(R, x, y), want, "point ({x},{y})");
        }
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        assert_eq!(direction(R, i32::MIN, 164), Dir::ScreenLeft);
        assert_eq!(direction(R, i32::MAX, i32::MAX), Dir::ScreenRight);
        let mut s = Slide::new((0, 0), (i32::MIN, 0), 1000);
        while !s.done() {
            s.step();
        }
        assert_eq!(s.position(), (i32::MIN, 0));
        assert_eq!(moving_state(R, i32::MIN, 0), Some("MovingRight"));
    }

    #[test]
    fn b_wl_2_b_wl_4_extreme_positions_do_not_overflow() {
        let o = [
            out(1, Rect::new(0, 0, 1000, 800)),
            out(2, Rect::new(1000, 0, 1000, 800)),
        ];
        let f = Rect::new(i32::MAX, 0, 100, 100);
        assert_eq!(output_for(f, &o).unwrap().id, 2);
        let f = Rect::new(i32::MIN, i32::MIN, 100, 100);
        assert_eq!(output_for(f, &o).unwrap().id, 1);
        assert_eq!(
            pull_on_output_change(Rect::new(i32::MAX, 0, 100, 100), &o),
            Some((900, 0))
        );
        assert_eq!(
            pull_inside(Rect::new(i32::MIN, i32::MAX, 100, 100), o[0].rect),
            (0, 700)
        );
    }

    #[test]
    fn r_dir_1_degenerate() {
        assert_eq!(direction(Rect::new(10, 0, 0, 10), 10, 5), Dir::ScreenRight);
        assert_eq!(direction(Rect::new(10, 0, 0, 10), 9, 5), Dir::ScreenLeft);
        assert_eq!(direction(Rect::new(0, 10, 10, 0), 5, 10), Dir::Down);
        assert_eq!(direction(Rect::new(0, 10, 10, 0), 5, 9), Dir::Up);
    }

    #[test]
    fn r_move_1_vectors() {
        let v = [
            ((0, 164), Some("MovingRight")),
            ((400, 164), Some("MovingLeft")),
            ((164, 0), Some("MovingUp")),
            ((164, 400), Some("MovingDown")),
            ((101, 164), Some("MovingDown")),
            ((220, 110), Some("MovingLeft")),
            ((150, 150), Some("MovingLeft")),
            ((100, 100), None),
        ];
        for ((x, y), want) in v {
            assert_eq!(moving_state(R, x, y), want, "target ({x},{y})");
        }
    }

    #[test]
    fn r_gest_1_vectors() {
        assert_eq!(gesture_state(R, 0, 164), "GesturingRight");
        assert_eq!(gesture_state(R, 400, 164), "GesturingLeft");
        assert_eq!(gesture_state(R, 164, 0), "GesturingUp");
        assert_eq!(gesture_state(R, 164, 400), "GesturingDown");
    }

    fn run(start: (i32, i32), target: (i32, i32), speed: u32) -> Vec<(u64, (i32, i32))> {
        let mut s = Slide::new(start, target, speed);
        let mut t = 0;
        let mut v = Vec::new();
        while !s.done() {
            t += s.period;
            v.push((t, s.step()));
        }
        v
    }

    #[test]
    fn b_move_1_slide_vectors() {
        assert_eq!(
            run((0, 0), (100, 0), 1000),
            vec![
                (40, (25, 0)),
                (80, (50, 0)),
                (120, (75, 0)),
                (160, (100, 0))
            ]
        );
        let v = run((0, 0), (1000, 500), 1000);
        assert_eq!(v.len(), 25);
        assert_eq!(v[0], (40, (40, 20)));
        assert_eq!(v[1], (80, (80, 40)));
        assert_eq!(*v.last().unwrap(), (1000, (1000, 500)));
        let v: Vec<_> = run((0, 0), (30, 200), 1000);
        let want = [
            (3, 25),
            (7, 50),
            (11, 75),
            (15, 100),
            (18, 125),
            (22, 150),
            (26, 175),
            (30, 200),
        ];
        for (i, (t, p)) in v.iter().enumerate() {
            assert_eq!(*t, 40 * (i as u64 + 1));
            assert_eq!(*p, want[i]);
        }
        assert_eq!(v.len(), 8);
    }

    #[test]
    fn b_move_1_period_floor() {
        assert_eq!(Slide::new((0, 0), (10, 0), 100).period, 10);
        assert_eq!(Slide::new((0, 0), (10, 0), 0).period, 10);
        // Negative direction, truncation toward zero on the minor axis.
        let v = run((0, 0), (-30, -200), 1000);
        assert_eq!(v[0].1, (-3, -25));
    }

    fn out(id: u32, r: Rect) -> OutputInfo {
        OutputInfo {
            id,
            rect: r,
            scale: 1.0,
        }
    }

    #[test]
    fn b_wl_2_output_choice() {
        let o = [
            out(1, Rect::new(0, 0, 1000, 800)),
            out(2, Rect::new(1000, 0, 1000, 800)),
        ];
        assert_eq!(output_for(Rect::new(950, 0, 128, 128), &o).unwrap().id, 2);
        assert_eq!(output_for(Rect::new(800, 0, 128, 128), &o).unwrap().id, 1);
        assert_eq!(output_for(Rect::new(-500, 0, 128, 128), &o).unwrap().id, 1);
        assert_eq!(output_for(Rect::new(2500, 0, 128, 128), &o).unwrap().id, 2);
    }

    #[test]
    fn b_wl_4_pull_inside() {
        let m = Rect::new(0, 0, 1000, 800);
        assert_eq!(pull_inside(Rect::new(950, 790, 100, 100), m), (900, 700));
        assert_eq!(pull_inside(Rect::new(-20, -5, 100, 100), m), (0, 0));
        // Larger than the output: left/top win.
        assert_eq!(pull_inside(Rect::new(10, 10, 2000, 100), m), (0, 10));
        let o = [out(1, Rect::new(0, 0, 1000, 800))];
        assert_eq!(
            pull_on_output_change(Rect::new(5000, 5000, 100, 100), &o),
            Some((900, 700))
        );
    }
}
