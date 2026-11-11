//! Command line of bentwired.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct OutputSpec {
    pub w: u32,
    pub h: u32,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
}

#[derive(Debug, Default)]
pub struct Args {
    pub headless: bool,
    pub test_interface: bool,
    pub characters: Vec<PathBuf>,
    pub outputs: Vec<OutputSpec>,
    pub draws: Option<PathBuf>,
    pub seed: Option<u64>,
    pub trace: Option<PathBuf>,
    pub dump_frames: Option<PathBuf>,
    pub no_tray: bool,
    pub no_audio: bool,
}

pub const USAGE: &str = "\
usage: bentwired [options]
  --characters DIR        prepend a character search root (repeatable)
  --headless              headless backend with fake clock (tests)
  --outputs WxH+X+Y[@S],...   virtual outputs (headless; default 1920x1080+0+0)
  --draws FILE            scripted random draws, whitespace-separated integers (headless)
  --seed N                seed for the random source
  --trace FILE            JSON lines of backend requests, sounds and signals (headless)
  --dump-frames DIR       PNG of the composed scene per output (headless)
  --test-interface        export org.bentwire.Test1 on the Wayland backend
  --no-tray               do not register a tray icon
  --no-audio              do not open an audio device
";

/// `1920x1080+0+0@2` (offsets may be negative: `+-1920+0` or `-1920+0`).
pub fn parse_output(s: &str) -> Result<OutputSpec, String> {
    let err = || format!("bad output {s:?}, expected WxH+X+Y[@scale]");
    let (geom, scale) = match s.split_once('@') {
        Some((g, sc)) => (g, sc.parse::<f64>().map_err(|_| err())?),
        None => (s, 1.0),
    };
    if scale <= 0.0 {
        return Err(err());
    }
    let (w, rest) = geom.split_once('x').ok_or_else(err)?;
    let idx = rest.find(['+', '-']).ok_or_else(err)?;
    let h = &rest[..idx];
    let rest = &rest[idx..];
    let mut nums = Vec::new();
    let mut cur = String::new();
    for c in rest.chars() {
        if (c == '+' || c == '-') && !cur.is_empty() && cur != "-" {
            nums.push(std::mem::take(&mut cur));
        }
        match c {
            '+' => {}
            _ => cur.push(c),
        }
    }
    nums.push(cur);
    if nums.len() != 2 {
        return Err(err());
    }
    Ok(OutputSpec {
        w: w.parse().map_err(|_| err())?,
        h: h.parse().map_err(|_| err())?,
        x: nums[0].parse().map_err(|_| err())?,
        y: nums[1].parse().map_err(|_| err())?,
        scale,
    })
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut val = |name: &str| {
            inline
                .clone()
                .or_else(|| it.next())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--headless" => a.headless = true,
            "--test-interface" => a.test_interface = true,
            "--no-tray" => a.no_tray = true,
            "--no-audio" => a.no_audio = true,
            "--characters" => a.characters.push(val("--characters")?.into()),
            "--outputs" => {
                for o in val("--outputs")?.split(',') {
                    a.outputs.push(parse_output(o)?);
                }
            }
            "--draws" => a.draws = Some(val("--draws")?.into()),
            "--seed" => {
                a.seed = Some(
                    val("--seed")?
                        .parse()
                        .map_err(|_| "--seed needs a number")?,
                )
            }
            "--trace" => a.trace = Some(val("--trace")?.into()),
            "--dump-frames" => a.dump_frames = Some(val("--dump-frames")?.into()),
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if a.outputs.is_empty() {
        a.outputs.push(parse_output("1920x1080+0+0").unwrap());
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_4_outputs() {
        let o = parse_output("1920x1080+0+0").unwrap();
        assert_eq!((o.w, o.h, o.x, o.y, o.scale), (1920, 1080, 0, 0, 1.0));
        let o = parse_output("800x600-800+-20@1.5").unwrap();
        assert_eq!((o.w, o.h, o.x, o.y, o.scale), (800, 600, -800, -20, 1.5));
        assert!(parse_output("800x600").is_err());
        assert!(parse_output("800x600+1+2@0").is_err());
        let a = parse(
            [
                "--headless",
                "--outputs=100x100+0+0,50x50+100+0@2",
                "--seed",
                "7",
            ]
            .map(String::from),
        )
        .unwrap();
        assert!(a.headless);
        assert_eq!(a.outputs.len(), 2);
        assert_eq!(a.seed, Some(7));
        assert!(parse(["--bogus".to_string()]).is_err());
    }
}
