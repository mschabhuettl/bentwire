//! Speak text processing (R-SPEAK-3, B-SPEAK-1, R-BAL-6, R-BAL-8).

use crate::api::{FontSpec, TextMetrics};

/// A parsed Speak part: words in order, with bookmarks attached to the word they precede
/// (`bookmarks_after` for marks after the last word).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpeakText {
    pub words: Vec<String>,
    /// `(word index, n)`: fire when word `index` appears; index == words.len() means at
    /// the end of speech.
    pub bookmarks: Vec<(usize, i32)>,
}

const KNOWN: [&str; 11] = [
    "Chr", "Ctx", "Emp", "Lst", "Map", "Mrk", "Pau", "Pit", "Rst", "Spd", "Vol",
];

/// Highest allowed bookmark number (R-Q-7: 0x7FFFFFFE and 0x7FFFFFFF are reserved).
pub const MAX_BOOKMARK: i64 = 2_147_483_645;

/// R-SPEAK-3: split alternatives at `|`.
pub fn alternatives(text: &str) -> Vec<&str> {
    text.split('|').collect()
}

fn quoted(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = s.split('"');
    it.next();
    while let Some(q) = it.next() {
        out.push(q.to_string());
        it.next();
    }
    out
}

/// B-SPEAK-1: parse tags. `Err` = invalid bookmark number (InvalidArgs).
pub fn parse(text: &str) -> Result<SpeakText, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut plain = String::new();
    let mut marks: Vec<(usize, i32)> = Vec::new(); // (byte position in plain, n)
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            let name: String = chars[i + 1..].iter().take(3).collect();
            if let Some(k) = KNOWN.iter().find(|k| k.eq_ignore_ascii_case(&name)) {
                // Read the tag body up to an unescaped backslash.
                let mut j = i + 4;
                let mut body = String::new();
                let mut closed = false;
                while j < chars.len() {
                    if chars[j] == '\\' {
                        if chars.get(j + 1) == Some(&'\\') {
                            body.push('\\');
                            j += 2;
                            continue;
                        }
                        closed = true;
                        break;
                    }
                    body.push(chars[j]);
                    j += 1;
                }
                if closed {
                    match *k {
                        "Map" => {
                            if let Some(b) = quoted(&body).get(1) {
                                plain.push_str(b);
                            }
                        }
                        "Mrk" => {
                            let v = body.trim().trim_start_matches('=').trim();
                            let n: i64 =
                                v.parse().map_err(|_| format!("invalid bookmark \"{v}\""))?;
                            if !(1..=MAX_BOOKMARK).contains(&n) {
                                return Err(format!("bookmark {n} out of range 1..{MAX_BOOKMARK}"));
                            }
                            marks.push((plain.len(), n as i32));
                        }
                        _ => {}
                    }
                    i = j + 1;
                    continue;
                }
            }
            plain.push('\\');
            i += 1;
            continue;
        }
        plain.push(chars[i]);
        i += 1;
    }
    // Words and the index of the word each mark precedes.
    let mut words = Vec::new();
    let mut starts = Vec::new();
    let mut cur = String::new();
    let mut cur_start = 0;
    for (pos, c) in plain.char_indices() {
        if c == ' ' || c == '\t' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
                starts.push(cur_start);
            }
        } else {
            if cur.is_empty() {
                cur_start = pos;
            }
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        words.push(cur);
        starts.push(cur_start);
    }
    let bookmarks = marks
        .into_iter()
        .map(|(p, n)| {
            (
                starts.iter().position(|&s| s >= p).unwrap_or(words.len()),
                n,
            )
        })
        .collect();
    Ok(SpeakText { words, bookmarks })
}

/// R-BAL-8: word interval in ms for config `speaking_speed` f.
pub fn word_interval(f: f64) -> u64 {
    let eff = (170.0 * f).clamp(50.0, 250.0);
    (352.0 * 170.0 / eff).floor() as u64
}

/// R-BAL-9: scroll duration, `60000 / effectiveSpeed` ms.
pub fn scroll_duration(f: f64) -> u64 {
    let eff = (170.0 * f).clamp(50.0, 250.0);
    (60000.0 / eff).floor() as u64
}

/// A wrapped line: `(word index, piece)` pairs; a long word may span lines.
pub type WrappedLine = Vec<(usize, String)>;

/// R-BAL-6 word wrap at `width` px.
pub fn wrap(
    words: &[String],
    width: i32,
    m: &dyn TextMetrics,
    font: &FontSpec,
) -> Vec<WrappedLine> {
    let space = m.text_width(" ", font);
    let mut lines: Vec<WrappedLine> = Vec::new();
    let mut line: WrappedLine = Vec::new();
    let mut used = 0;
    for (wi, w) in words.iter().enumerate() {
        let ww = m.text_width(w, font);
        let need = if line.is_empty() {
            ww
        } else {
            used + space + ww
        };
        if need <= width {
            used = need;
            line.push((wi, w.clone()));
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        if ww <= width {
            used = ww;
            line.push((wi, w.clone()));
            continue;
        }
        // Break a word wider than a line at the longest prefix that fits (widths are
        // additive, B-REN-1, so one pass over the characters).
        let mut piece = String::new();
        let mut pw = 0;
        for ch in w.chars() {
            let cw = m.text_width(ch.encode_utf8(&mut [0; 4]), font);
            if !piece.is_empty() && pw + cw > width {
                lines.push(vec![(wi, std::mem::take(&mut piece))]);
                pw = 0;
            }
            piece.push(ch);
            pw += cw;
        }
        used = pw;
        line.push((wi, piece));
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Fixed-width fake metrics: every char `cw` px, line height `lh`.
    pub struct Fixed {
        pub cw: i32,
        pub lh: i32,
    }

    impl TextMetrics for Fixed {
        fn line_height(&self, _: &FontSpec) -> i32 {
            self.lh
        }
        fn ave_char_width(&self, _: &FontSpec) -> i32 {
            self.cw
        }
        fn text_width(&self, t: &str, _: &FontSpec) -> i32 {
            t.chars().count() as i32 * self.cw
        }
    }

    pub fn font() -> FontSpec {
        FontSpec {
            family: "Test".into(),
            px: 13,
            cell: false,
            bold: false,
            italic: false,
        }
    }

    fn w(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn r_speak_3_alternatives() {
        assert_eq!(alternatives("a|b c|d"), vec!["a", "b c", "d"]);
        assert_eq!(alternatives("plain"), vec!["plain"]);
    }

    #[test]
    fn b_speak_1_tags() {
        let t = parse("Hello \\Mrk=5\\big \\emp\\world").unwrap();
        assert_eq!(t.words, w(&["Hello", "big", "world"]));
        assert_eq!(t.bookmarks, vec![(1, 5)]);
        let t = parse("\\Map=\"spoken\"=\"shown text\"\\ end \\mrk=2\\").unwrap();
        assert_eq!(t.words, w(&["shown", "text", "end"]));
        assert_eq!(t.bookmarks, vec![(3, 2)]);
        // Unknown tag: literal backslash.
        let t = parse("a\\xyz\\ b").unwrap();
        assert_eq!(t.words, w(&["a\\xyz\\", "b"]));
        // \\ inside a tag is a literal backslash, not the end.
        let t = parse("\\Map=\"s\"=\"a\\\\b\"\\").unwrap();
        assert_eq!(t.words, w(&["a\\b"]));
        // Other known tags are removed.
        let t = parse("\\Pau=100\\x\\Spd=200\\ y").unwrap();
        assert_eq!(t.words, w(&["x", "y"]));
    }

    #[test]
    fn b_speak_1_r_q_7_bookmark_range() {
        assert!(parse("\\Mrk=0\\").is_err());
        assert!(parse("\\Mrk=2147483646\\").is_err());
        assert!(parse("\\Mrk=2147483647\\").is_err());
        assert!(parse("\\Mrk=x\\").is_err());
        assert_eq!(
            parse("\\Mrk=2147483645\\").unwrap().bookmarks,
            vec![(0, 2147483645)]
        );
        assert_eq!(parse("\\Mrk=1\\").unwrap().bookmarks, vec![(0, 1)]);
    }

    #[test]
    fn r_bal_8_r_ui_5_interval() {
        assert_eq!(word_interval(1.0), 352);
        // 170 * 0.5 = 85: 59840 / 85 = 704.
        assert_eq!(word_interval(0.5), 704);
        // 170 * 1.5 = 255 clamps to 250: 59840 / 250 = 239.36.
        assert_eq!(word_interval(1.5), 239);
        assert_eq!(word_interval(0.1), 1196);
        assert_eq!(scroll_duration(1.0), 352);
    }

    #[test]
    fn r_bal_6_word_wrap() {
        let m = Fixed { cw: 10, lh: 13 };
        let f = font();
        // Width 100 = 10 chars.
        let l = wrap(&w(&["aaaa", "bbbb", "cc", "d"]), 100, &m, &f);
        let s: Vec<Vec<&str>> = l
            .iter()
            .map(|x| x.iter().map(|p| p.1.as_str()).collect())
            .collect();
        assert_eq!(s, vec![vec!["aaaa", "bbbb"], vec!["cc", "d"]]);
        // Exactly fitting: "aaaa bbbbb" = 10 chars.
        let l = wrap(&w(&["aaaa", "bbbbb"]), 100, &m, &f);
        assert_eq!(l.len(), 1);
        // A word wider than a line breaks at the longest prefix.
        let l = wrap(&w(&["x", "abcdefghijklmnopqrstuvwxy", "z"]), 100, &m, &f);
        let s: Vec<Vec<&str>> = l
            .iter()
            .map(|x| x.iter().map(|p| p.1.as_str()).collect())
            .collect();
        assert_eq!(
            s,
            vec![
                vec!["x"],
                vec!["abcdefghij"],
                vec!["klmnopqrst"],
                vec!["uvwxy", "z"]
            ]
        );
        // Tab breaks (B-SPEAK-1 word split).
        assert_eq!(parse("a\tb").unwrap().words, w(&["a", "b"]));
        // A very long word is linear, not cubic.
        let long = "x".repeat(200_000);
        let l = wrap(&[long], 100, &m, &f);
        assert_eq!(l.len(), 20_000);
    }
}
