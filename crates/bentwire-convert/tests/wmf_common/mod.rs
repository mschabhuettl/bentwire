//! Shared helpers for the WMF rasteriser tests: indexed PNG encode/decode.
#![allow(dead_code)]

use bentwire_convert::wmf::Canvas;

/// Indexed PNG with `tRNS` making only `canvas.transparent` transparent.
pub fn encode_png(c: &Canvas) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, c.width, c.height);
        e.set_color(png::ColorType::Indexed);
        e.set_depth(png::BitDepth::Eight);
        e.set_palette(c.palette.concat());
        let mut trns = vec![255u8; c.palette.len()];
        trns[usize::from(c.transparent)] = 0;
        e.set_trns(trns);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&c.pixels).unwrap();
    }
    out
}

/// Decode an indexed 8-bit PNG to (width, height, indices, palette).
pub fn decode_png(b: &[u8]) -> (u32, u32, Vec<u8>, Vec<[u8; 3]>) {
    let d = png::Decoder::new(std::io::Cursor::new(b));
    let mut r = d.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    let pal = r
        .info()
        .palette
        .as_ref()
        .unwrap()
        .chunks(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    (info.width, info.height, buf, pal)
}
