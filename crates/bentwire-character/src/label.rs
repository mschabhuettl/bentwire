//! Display labels (spec 3.14 A-LABEL-1): "<name> (<product>)" for every character, so that
//! characters of the same name in different products stay apart in the character list, the
//! gallery dialog, the tray menu and a pack's README gallery.
//!
//! The product comes from the manifest's `source.format` ([`PRODUCTS`]) unless the pack's
//! `pack.toml` overrides it per id; a pack may also override the English name:
//!
//! ```toml
//! [labels.some-id]
//! name = "English name"
//! product = "Product (qualifier)"
//! ```

use std::collections::BTreeMap;
use std::path::Path;

/// The product of each source format.
pub const PRODUCTS: [(&str, &str); 4] = [
    ("acs-2.0", "Microsoft Agent 2.0"),
    ("acs-1.5", "Microsoft Agent 1.5"),
    ("act-2.2", "Office 97"),
    ("act-1.0", "Microsoft Bob"),
];

/// The product of a `source.format` value, if it is one of [`PRODUCTS`].
pub fn product_of_format(format: &str) -> Option<&'static str> {
    PRODUCTS.iter().find(|(f, _)| *f == format).map(|(_, p)| *p)
}

/// Per-id overrides from a pack's `pack.toml` (`[labels.<id>]`, keys `name` and `product`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackLabels {
    /// id -> (name, product); empty strings count as absent.
    pub labels: BTreeMap<String, (Option<String>, Option<String>)>,
}

impl PackLabels {
    pub fn parse(text: &str) -> Result<Self, String> {
        let doc: toml::Table = text.parse().map_err(|e| format!("pack.toml: {e}"))?;
        let mut labels = BTreeMap::new();
        let Some(t) = doc.get("labels") else {
            return Ok(Self { labels });
        };
        let t = t.as_table().ok_or("pack.toml: labels is not a table")?;
        for (id, v) in t {
            let v = v
                .as_table()
                .ok_or_else(|| format!("pack.toml: labels.{id} is not a table"))?;
            let field = |k: &str| -> Result<Option<String>, String> {
                match v.get(k) {
                    None => Ok(None),
                    Some(s) => s
                        .as_str()
                        .map(|s| Some(s.to_string()).filter(|s| !s.is_empty()))
                        .ok_or_else(|| format!("pack.toml: labels.{id}.{k} is not a string")),
                }
            };
            labels.insert(id.to_ascii_lowercase(), (field("name")?, field("product")?));
        }
        Ok(Self { labels })
    }

    /// The overrides in `dir/pack.toml`; none when there is no such file.
    pub fn of_pack(dir: &Path) -> Result<Self, String> {
        let file = dir.join("pack.toml");
        if !file.is_file() {
            return Ok(Self::default());
        }
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", file.display()))
    }

    /// The overrides of the pack holding the character at `path` (a `.bwc` file or a
    /// folder), from its parent's `pack.toml`.
    pub fn for_character(path: &Path) -> Result<Self, String> {
        path.parent().map_or(Ok(Self::default()), Self::of_pack)
    }
}

fn primary(lang: &str) -> &str {
    lang.split(['-', '_']).next().unwrap_or("")
}

fn is_en(lang: &str) -> bool {
    primary(lang).eq_ignore_ascii_case("en")
}

/// The English name and the product of character `id`. `info` holds `(lang, name)` per
/// `[[character.info]]` entry. The name is the pack's override, else the English name
/// (B-CHAR-3 for `en`), else the first non-empty name in any language, else the id: never
/// empty.
pub fn name_and_product(
    id: &str,
    info: &[(&str, &str)],
    format: Option<&str>,
    pack: &PackLabels,
) -> (String, Option<String>) {
    let (o_name, o_product) = pack
        .labels
        .get(&id.to_ascii_lowercase())
        .cloned()
        .unwrap_or_default();
    let english = info
        .iter()
        .find(|(l, n)| l.eq_ignore_ascii_case("en") && !n.is_empty())
        .or_else(|| info.iter().rev().find(|(l, _)| is_en(l)))
        .map(|(_, n)| *n)
        .filter(|n| !n.is_empty());
    let name = o_name
        .or_else(|| english.map(str::to_string))
        .or_else(|| {
            info.iter()
                .find(|(_, n)| !n.is_empty())
                .map(|(_, n)| n.to_string())
        })
        .unwrap_or_else(|| id.to_string());
    let product = o_product.or_else(|| format.and_then(product_of_format).map(str::to_string));
    (name, product)
}

/// "<name> (<product>)", or the name alone without a product. A qualifier in the product
/// joins with a comma: "Name (Product, qualifier)" for product "Product (qualifier)".
pub fn label(name: &str, product: Option<&str>) -> String {
    match product {
        None => name.to_string(),
        Some(p) => match p.strip_suffix(')').and_then(|q| q.split_once(" (")) {
            Some((head, qualifier)) => format!("{name} ({head}, {qualifier})"),
            None => format!("{name} ({p})"),
        },
    }
}

/// The `[[character.info]]` entries `(lang, name, description)` with every name labelled,
/// as the daemon shows them (B-CHAR-3 then picks one per UI language). English entries get
/// the name of [`name_and_product`]; other entries keep theirs unless it is empty. Without
/// an English entry one is added, so no UI language ever gets an empty name.
pub fn label_info(
    id: &str,
    info: &[(String, String, String)],
    format: Option<&str>,
    pack: &PackLabels,
) -> Vec<(String, String, String)> {
    let names: Vec<(&str, &str)> = info
        .iter()
        .map(|(l, n, _)| (l.as_str(), n.as_str()))
        .collect();
    let (name, product) = name_and_product(id, &names, format, pack);
    let product = product.as_deref();
    let mut out: Vec<(String, String, String)> = info
        .iter()
        .map(|(l, n, d)| {
            let n = if is_en(l) || n.is_empty() { &name } else { n };
            (l.clone(), label(n, product), d.clone())
        })
        .collect();
    if !info.iter().any(|(l, _, _)| is_en(l)) {
        // The description of the entry the name came from, if any.
        let d = info
            .iter()
            .find(|(_, n, _)| *n == name)
            .map(|(_, _, d)| d.clone())
            .unwrap_or_default();
        out.push(("en".to_string(), label(&name, product), d));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(v: &[(&str, &str)]) -> Vec<(String, String, String)> {
        v.iter()
            .map(|(l, n)| (l.to_string(), n.to_string(), format!("d-{n}")))
            .collect()
    }

    #[test]
    fn a_label_1_product_from_format_and_qualifier() {
        let p = PackLabels::default();
        let l = label_info("x", &info(&[("en", "Alpha")]), Some("acs-1.5"), &p);
        assert_eq!(l[0].1, "Alpha (Microsoft Agent 1.5)");
        assert_eq!(
            label("A", Some("Office 97 (static)")),
            "A (Office 97, static)"
        );
        assert_eq!(label("A", None), "A");
        // No known format: the name alone.
        let l = label_info("x", &info(&[("en", "Alpha")]), None, &p);
        assert_eq!(l, info(&[("en", "Alpha")]));
    }

    #[test]
    fn a_label_1_never_empty() {
        let p = PackLabels::default();
        // Only another language: its name, and an English entry is added.
        let l = label_info("x", &info(&[("ja", "Beta")]), Some("act-2.2"), &p);
        assert_eq!(l.len(), 2);
        assert_eq!(
            l[1],
            ("en".into(), "Beta (Office 97)".into(), "d-Beta".into())
        );
        // Empty names everywhere, or no entries: the id.
        let l = label_info("some-id", &info(&[("en", ""), ("de", "")]), None, &p);
        assert_eq!(l[0].1, "some-id");
        assert_eq!(l[1].1, "some-id");
        let l = label_info("some-id", &[], Some("act-1.0"), &p);
        assert_eq!(
            l,
            vec![("en".into(), "some-id (Microsoft Bob)".into(), String::new())]
        );
    }

    #[test]
    fn a_label_1_pack_overrides() {
        let p = PackLabels::parse(
            "name = \"p\"\n[labels.a-id]\nname = \"Gamma\"\nproduct = \"Prod (q)\"\n\
             [labels.b-id]\nproduct = \"Other\"\n[labels.c-id]\nname = \"\"\n",
        )
        .unwrap();
        let l = label_info(
            "a-id",
            &info(&[("en-US", "Unnamed"), ("de", "Delta"), ("fr", "")]),
            Some("acs-2.0"),
            &p,
        );
        let names: Vec<&str> = l.iter().map(|e| e.1.as_str()).collect();
        assert_eq!(
            names,
            ["Gamma (Prod, q)", "Delta (Prod, q)", "Gamma (Prod, q)"]
        );
        let l = label_info("b-id", &info(&[("en", "Eps")]), Some("acs-2.0"), &p);
        assert_eq!(l[0].1, "Eps (Other)");
        // An empty override is no override.
        let l = label_info("c-id", &info(&[("en", "Zeta")]), Some("acs-2.0"), &p);
        assert_eq!(l[0].1, "Zeta (Microsoft Agent 2.0)");
        assert!(PackLabels::parse("labels = 1").is_err());
        assert!(PackLabels::parse("[labels.x]\nname = 2").is_err());
        assert_eq!(
            PackLabels::parse("name = \"q\"").unwrap(),
            PackLabels::default()
        );
    }
}
