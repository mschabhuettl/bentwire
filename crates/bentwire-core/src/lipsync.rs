//! Lip sync pure functions (R-LIP-1, B-LIP-1). Not driven in v1 (B-LIP-2).

/// The 116-entry phoneme table (acs-format 5.4), by mouth type.
const TABLE: [&str; 7] = [
    "\u{0} _bmpɓɸʘβ",
    "finvɨɩɪɯɱʇʋ",
    "cdeklqstzðŋɖɗɘɟɡɢɬɭɮɲɳɺɽɾɿʂʈʐʔʗʣʥʦʨθ",
    "jxçɑəɛɠɣɥɧɰɴʀʁʆʌʎʓχ",
    "ahæħɜɦʕʖ",
    "œɒɔɕɚɝɞɤɫɶɹɻʃʄʅʑʒʤʧ",
    "oruwyøɵɷɼʉʊʍʏ",
];

/// Mouth type for a phoneme, or `None` if it is not in the table.
pub fn mouth_for_phoneme(c: char) -> Option<u8> {
    TABLE
        .iter()
        .position(|row| row.contains(c))
        .map(|i| i as u8)
}

/// R-LIP-1 stateful step: an unknown phoneme keeps the previous mouth (initially none).
pub fn next_mouth(previous: Option<u8>, c: char) -> Option<u8> {
    mouth_for_phoneme(c).or(previous)
}

/// B-LIP-1: amplitude quantisation to a representative phoneme; `None` = silence.
pub fn phoneme_for_amplitude(amplitude: Option<u8>) -> char {
    match amplitude {
        None => 'm',
        Some(a) if a < 0x10 => 'b',
        Some(a) if a < 0x30 => 'f',
        Some(a) if a < 0x50 => 'e',
        Some(a) if a < 0x70 => 'ɑ',
        Some(_) => 'ɜ',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r_lip_1_table_size_and_unique() {
        let all: Vec<char> = TABLE.iter().flat_map(|r| r.chars()).collect();
        assert_eq!(all.len(), 116);
        let mut s = all.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), 116);
    }

    #[test]
    fn r_lip_1_vectors() {
        let v = [
            (' ', 0),
            ('m', 0),
            ('f', 1),
            ('n', 1),
            ('t', 2),
            ('\u{259}', 3),
            ('a', 4),
            ('\u{283}', 5),
            ('o', 6),
            ('w', 6),
        ];
        for (c, m) in v {
            assert_eq!(mouth_for_phoneme(c), Some(m), "{c:?}");
        }
        assert_eq!(next_mouth(None, 'Q'), None);
        assert_eq!(next_mouth(Some(4), 'Q'), Some(4));
        assert_eq!(next_mouth(Some(4), 'o'), Some(6));
    }

    #[test]
    fn b_lip_1_quantisation() {
        assert_eq!(phoneme_for_amplitude(Some(0)), 'b');
        assert_eq!(phoneme_for_amplitude(Some(0x0F)), 'b');
        assert_eq!(phoneme_for_amplitude(Some(0x10)), 'f');
        assert_eq!(phoneme_for_amplitude(Some(0x2F)), 'f');
        assert_eq!(phoneme_for_amplitude(Some(0x30)), 'e');
        assert_eq!(phoneme_for_amplitude(Some(0x50)), 'ɑ');
        assert_eq!(phoneme_for_amplitude(Some(0x6F)), 'ɑ');
        assert_eq!(phoneme_for_amplitude(Some(0x70)), 'ɜ');
        assert_eq!(phoneme_for_amplitude(Some(0xFF)), 'ɜ');
        assert_eq!(phoneme_for_amplitude(None), 'm');
    }
}
