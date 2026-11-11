//! Cross-reference rules a schema cannot express (the prototype's `validate.py`,
//! character-format 4).

use std::collections::HashSet;

use crate::model::*;

/// Every rule violation of a parsed manifest, as messages. Empty = valid. Unknown keys that do
/// not start with `x-` are errors here (character-format 4).
pub fn validate(m: &Manifest) -> Vec<String> {
    let mut errs: Vec<String> = m
        .unknown_keys
        .iter()
        .map(|k| format!("unknown key {k}"))
        .collect();
    let missing: HashSet<String> = m
        .missing_animations()
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let mut lower = HashSet::new();
    for a in &m.animations {
        if !lower.insert(a.name.to_ascii_lowercase()) {
            errs.push(format!(
                "animation name {:?} not unique (ASCII case ignored)",
                a.name
            ));
        }
    }
    let known = |n: &str| {
        let l = n.to_ascii_lowercase();
        lower.contains(&l) || missing.contains(&l)
    };
    let mut ids = HashSet::new();
    for s in &m.sheets {
        if !ids.insert(s.id.as_str()) {
            errs.push(format!("sheet id {:?} not unique", s.id));
        }
        if let Some(sz) = &s.sizes {
            if sz.len() != s.count as usize {
                errs.push(format!(
                    "sheet {}: sizes has {} entries, count is {}",
                    s.id,
                    sz.len(),
                    s.count
                ));
            }
            for (n, [w, h]) in sz.iter().enumerate() {
                if *w > s.cell[0] || *h > s.cell[1] {
                    errs.push(format!("sheet {}: sizes[{n}] larger than the cell", s.id));
                }
            }
        }
    }
    let ref_ok = |l: &Layer| {
        m.sheet(&l.image.sheet)
            .is_some_and(|s| l.image.index < s.count)
    };
    let sound_ids: HashSet<&str> = m.sounds.iter().map(|(k, _)| k.as_str()).collect();
    if sound_ids.len() != m.sounds.len() {
        errs.push("sound ids not unique".into());
    }
    for a in &m.animations {
        let nf = a.frames.len() as u32;
        if let Some(r) = &a.return_animation
            && !known(r)
        {
            errs.push(format!("{}: return animation {r:?} missing", a.name));
        }
        if m.family == Family::Office97 && (a.transition.is_some() || a.return_animation.is_some())
        {
            errs.push(format!("{}: transition/return are agent-only keys", a.name));
        }
        for (i, f) in a.frames.iter().enumerate() {
            let w = format!("{}[{i}]", a.name);
            let all_layers = f
                .layers
                .iter()
                .chain(f.mouths.iter().map(|mo| &mo.layer))
                .chain(f.mouths.iter().flat_map(|mo| mo.base.iter().flatten()));
            for l in all_layers {
                if !ref_ok(l) {
                    errs.push(format!(
                        "{w}: bad image reference {}:{}",
                        l.image.sheet, l.image.index
                    ));
                }
            }
            if let Some(p) = f.pose
                && !m.poses.contains_key(&p)
            {
                errs.push(format!("{w}: unknown pose {p}"));
            }
            if let Some(s) = &f.sound
                && !sound_ids.contains(s.as_str())
            {
                errs.push(format!("{w}: unknown sound {s:?}"));
            }
            if let Some(Exit::Frame(x)) = f.exit
                && x >= nf
            {
                errs.push(format!("{w}: exit target out of range"));
            }
            if f.branch.iter().any(|b| b.target() >= nf) {
                errs.push(format!("{w}: branch target out of range"));
            }
            match m.family {
                Family::Agent => {
                    let mut sum = 0u32;
                    let mut ok = true;
                    for b in &f.branch {
                        match *b {
                            Branch::Random { p, .. } if (1..=100).contains(&p) => sum += p,
                            _ => ok = false,
                        }
                    }
                    if !ok || sum > 100 {
                        errs.push(format!(
                            "{w}: agent branch probabilities must be 1..100 and sum to at most 100"
                        ));
                    }
                    if f.sound_arg.is_some() {
                        errs.push(format!("{w}: office97-only key sound_arg"));
                    }
                }
                Family::Office97 => {
                    if f.exit.is_some() || !f.mouths.is_empty() {
                        errs.push(format!("{w}: agent-only key"));
                    }
                }
            }
        }
    }
    for (s, list) in &m.states {
        for a in list {
            if !known(a) {
                errs.push(format!("state {s}: unknown animation {a:?}"));
            }
        }
    }
    for (p, layers) in &m.poses {
        if !layers.iter().all(ref_ok) {
            errs.push(format!("pose {p}: bad image reference"));
        }
    }
    match (&m.office97, m.family) {
        (Some(_), Family::Agent) | (None, Family::Office97) => {
            errs.push("[office97] table must be present exactly for family office97".into())
        }
        (Some(o), Family::Office97) => {
            if !known(&o.idle) && m.state(&o.idle).is_none() {
                errs.push(format!(
                    "office97.idle {:?} is neither a state nor an animation",
                    o.idle
                ));
            }
            for (from, to, st) in &o.transitions {
                if m.state(st).is_none() && !known(st) {
                    errs.push(format!(
                        "office97.transitions [{from}, {to}, {st:?}]: unknown state"
                    ));
                }
            }
        }
        _ => {}
    }
    if let Some(i) = &m.icon
        && i.file.is_empty()
    {
        errs.push("icon.file empty".into());
    }
    errs
}
