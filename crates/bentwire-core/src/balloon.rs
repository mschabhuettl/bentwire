//! Agent speech balloon geometry (R-BAL-1..5, B-BAL-3).

use crate::api::{Rect, TailSide};

pub const TAIL_LEN: i32 = 16;
pub const TAIL_HALF: i32 = 4;

/// R-BAL-1: lines 0 means 2, chars per line 0 means 28.
pub fn effective_lines_cpl(lines: u32, cpl: u32) -> (u32, u32) {
    (
        if lines == 0 { 2 } else { lines },
        if cpl == 0 { 28 } else { cpl },
    )
}

/// R-BAL-3: body size `(W, H)` for `lines` lines.
pub fn body_size(cpl: u32, ave_char: i32, line_height: i32, lines: u32, max_w: i32) -> (i32, i32) {
    let w = (cpl as i32 * ave_char).min(max_w);
    (w, lines as i32 * (line_height + 2) + 18)
}

/// R-BAL-3 SizeToText: wrapped line count capped at 128, reduced until H fits `max_h`.
pub fn size_to_text_lines(wrapped: usize, line_height: i32, max_h: i32) -> u32 {
    let mut n = wrapped.clamp(1, 128) as u32;
    while n > 1 && n as i32 * (line_height + 2) + 18 > max_h {
        n -= 1;
    }
    n
}

/// Placement result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub window: Rect,
    pub body: Rect,
    pub side: TailSide,
    /// Tip, then the base points.
    pub tail: [(i32, i32); 3],
}

/// R-BAL-5 / B-BAL-3 placement. `c` = character frame, `m` = output rect, `anchor` =
/// mouth point or frame centre (global, unclamped).
pub fn place(c: Rect, m: Rect, w: i32, h: i32, anchor: (i32, i32)) -> Placement {
    let ax = anchor.0.clamp(m.x, (m.right() - 1).max(m.x));
    let ay = anchor.1.clamp(m.y, (m.bottom() - 1).max(m.y));
    let top = c.y - TAIL_LEN - h;
    if top >= m.y {
        let mut left = c.x + (c.w - w) / 2;
        left = left.min(m.right() - w - 1);
        left = left.max(m.x);
        let window = Rect::new(left, top, w, h + TAIL_LEN);
        let body = Rect::new(left, top, w, h);
        let lo = left + 10 + TAIL_HALF;
        let tx = ax.clamp(lo, (left + w - 10 - TAIL_HALF).max(lo));
        return Placement {
            window,
            body,
            side: TailSide::Bottom,
            tail: [
                (tx, top + h + TAIL_LEN),
                (tx - TAIL_HALF, top + h),
                (tx + TAIL_HALF, top + h),
            ],
        };
    }
    let top = c.y.max(m.y);
    let lo = top + 12 + TAIL_HALF;
    let ty = ay.clamp(lo, (top + h - 12 - TAIL_HALF).max(lo));
    if c.x > (m.right() - m.x) / 2 {
        let left = c.x - TAIL_LEN - w;
        Placement {
            window: Rect::new(left, top, w + TAIL_LEN, h),
            body: Rect::new(left, top, w, h),
            side: TailSide::Right,
            tail: [
                (left + w + TAIL_LEN, ty),
                (left + w, ty - TAIL_HALF),
                (left + w, ty + TAIL_HALF),
            ],
        }
    } else {
        let left = c.right();
        Placement {
            window: Rect::new(left, top, w + TAIL_LEN, h),
            body: Rect::new(left + TAIL_LEN, top, w, h),
            side: TailSide::Left,
            tail: [
                (left, ty),
                (left + TAIL_LEN, ty - TAIL_HALF),
                (left + TAIL_LEN, ty + TAIL_HALF),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: Rect = Rect::new(0, 0, 1024, 768);

    fn centre(c: Rect) -> (i32, i32) {
        (c.x + c.w / 2, c.y + c.h / 2)
    }

    #[test]
    fn b_bal_3_r_bal_4_r_bal_5_vectors() {
        let c = Rect::new(448, 400, 128, 128);
        let p = place(c, M, 200, 54, centre(c));
        assert_eq!(p.window, Rect::new(412, 330, 200, 70));
        assert_eq!(p.side, TailSide::Bottom);
        // R-BAL-4: tail length 16, half width 4, tip at the anchor x.
        assert_eq!(p.tail, [(512, 400), (508, 384), (516, 384)]);
        let c = Rect::new(900, 10, 128, 128);
        let p = place(c, M, 200, 54, centre(c));
        assert_eq!((p.window.x, p.window.y), (684, 10));
        assert_eq!(p.side, TailSide::Right);
        let c = Rect::new(10, 10, 128, 128);
        let p = place(c, M, 200, 54, centre(c));
        assert_eq!(p.window.x, 138);
        assert_eq!(p.side, TailSide::Left);
        assert_eq!(p.body.x, 154);
    }

    #[test]
    fn r_bal_5_clamps() {
        // Left edge clamp above.
        let c = Rect::new(-50, 400, 128, 128);
        let p = place(c, M, 200, 54, centre(c));
        assert_eq!(p.window.x, 0);
        // Right edge: left + W < M.right.
        let c = Rect::new(950, 400, 128, 128);
        let p = place(c, M, 200, 54, centre(c));
        assert_eq!(p.window.x, 1024 - 200 - 1);
        // Tail x range on a bottom tail.
        assert!(p.tail[0].0 <= p.window.x + 200 - 14);
        // Side tail y range, and half-width compare (not the centre) on a secondary output.
        let m2 = Rect::new(1024, 0, 1024, 768);
        let c = Rect::new(1100, 5, 128, 128);
        let p = place(c, m2, 200, 54, (1164, 5));
        // 1100 > 1024 / 2, so left of the character even though it is left on its output.
        assert_eq!(p.side, TailSide::Right);
        assert_eq!(p.tail[0].1, 5 + 12 + 4);
        // Anchor clamped into M.
        let c = Rect::new(448, 400, 128, 128);
        let p = place(c, M, 200, 54, (5000, 464));
        assert_eq!(p.tail[0].0, 412 + 200 - 14);
    }

    #[test]
    fn r_bal_5_tiny_balloon_does_not_panic() {
        let c = Rect::new(448, 400, 128, 128);
        let p = place(c, M, 21, 20, centre(c));
        assert_eq!(p.side, TailSide::Bottom);
        let c = Rect::new(448, 0, 128, 128);
        let p = place(c, M, 21, 20, centre(c));
        assert_eq!(p.side, TailSide::Left);
    }

    #[test]
    fn r_bal_1_r_bal_3_size() {
        assert_eq!(effective_lines_cpl(0, 0), (2, 28));
        assert_eq!(body_size(28, 7, 13, 2, 10000), (196, 2 * 15 + 18));
        assert_eq!(body_size(28, 7, 13, 2, 100).0, 100);
        assert_eq!(size_to_text_lines(5, 13, 1000), 5);
        assert_eq!(size_to_text_lines(500, 13, 100000), 128);
        // 5 lines = 93 px; fit in 70 px -> 3 lines (63 px).
        assert_eq!(size_to_text_lines(5, 13, 70), 3);
    }
}
