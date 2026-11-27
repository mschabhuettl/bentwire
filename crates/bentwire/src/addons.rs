//! `bentwire addons` (spec B-ADDON-2, B-ADDON-3): add-ons in `$XDG_DATA_HOME/bentwire/addons`,
//! the enabled list in `$XDG_CONFIG_HOME/bentwire/addons.toml`.

use crate::{AGENT_IFACE, AGENT_PATH, BUS_NAME, Fail, Opts, curl, fail, sha256_hex, xdg_dir};
use bentwire_character::addon::{self, Addon, Kind, MANIFEST};
use std::cell::Cell;
use std::io::{Read, Seek};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

const USAGE: &str =
    "expected list, install <folder|archive|url> [--sha256 HEX], enable, disable or remove <name>";

/// B-ADDON-2: an archive or download has at most `packed` bytes, unpacks to at most
/// `unpacked` bytes and has at most `entries` entries.
struct Limits {
    packed: u64,
    unpacked: u64,
    entries: usize,
}

const LIMITS: Limits = Limits {
    packed: 256 << 20,
    unpacked: 1 << 30,
    entries: 10_000,
};

/// `n` bytes for a message: in MiB when it is a whole number of them.
fn size(n: u64) -> String {
    if n >= 1 << 20 && n.is_multiple_of(1 << 20) {
        format!("{} MiB", n >> 20)
    } else {
        format!("{n} bytes")
    }
}

fn too_large(what: &str, limit: u64) -> String {
    format!(
        "{what} more than {} (the limit for add-on archives)",
        size(limit)
    )
}

/// A reader that fails as soon as more than `left` bytes came through it.
struct Capped<R> {
    r: R,
    left: u64,
    what: &'static str,
    limit: u64,
}

impl<R: Read> Read for Capped<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.r.read(buf)?;
        if n as u64 > self.left {
            return Err(std::io::Error::other(too_large(self.what, self.limit)));
        }
        self.left -= n as u64;
        Ok(n)
    }
}

/// B-ADDON-2: what the tar crate may read before it returns an entry: the entry's header
/// blocks and its GNU long name and pax extension data, which it holds in memory.
const TAR_HEADERS: u64 = 1 << 20;

/// A reader that fails when more than `left` bytes come through it; the caller sets `left`.
struct Budget<R> {
    r: R,
    left: Rc<Cell<u64>>,
}

impl<R: Read> Read for Budget<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.r.read(buf)? as u64;
        if n > self.left.get() {
            return Err(std::io::Error::other(too_large(
                "an entry's tar headers are",
                TAR_HEADERS,
            )));
        }
        self.left.set(self.left.get() - n);
        Ok(n as usize)
    }
}

pub fn run(args: &[String]) -> Result<(), Fail> {
    let o = Opts::parse(args, &[], &["--sha256"])?;
    let pos: Vec<&str> = o.positional.iter().map(String::as_str).collect();
    let sha = o.value("--sha256");
    if sha.is_some() && pos.first() != Some(&"install") {
        return Err(Fail::Usage("--sha256 is for install".into()));
    }
    match pos.as_slice() {
        [] | ["list"] => list(),
        ["install", src] => install(src, sha),
        ["enable", name] => set_enabled(name, true),
        ["disable", name] => set_enabled(name, false),
        ["remove", name] => remove(name),
        _ => Err(Fail::Usage(USAGE.into())),
    }
}

fn addons_dir() -> PathBuf {
    xdg_dir("XDG_DATA_HOME", ".local/share").join("bentwire/addons")
}

fn list_file() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("bentwire/addons.toml")
}

fn enabled() -> Result<Vec<String>, Fail> {
    addon::read_enabled(&list_file()).map_err(fail)
}

fn save_enabled(names: &[String]) -> Result<(), Fail> {
    let f = list_file();
    addon::write_enabled(&f, names).map_err(|e| fail(format!("{}: {e}", f.display())))
}

fn list() -> Result<(), Fail> {
    let on = enabled()?;
    for (_, a) in addon::installed(&addons_dir()) {
        match a {
            Ok(a) => {
                let state = if on.contains(&a.name) {
                    "enabled"
                } else {
                    "disabled"
                };
                outln!("{}\t{}\t{}\t{state}", a.name, a.kind.as_str(), a.version);
            }
            Err(e) => eprintln!("bentwire: {e}"),
        }
    }
    Ok(())
}

/// The folder of the installed add-on `name`.
fn installed_dir(name: &str) -> Result<PathBuf, Fail> {
    let d = addons_dir().join(name);
    if addon::valid_name(name) && d.is_dir() {
        Ok(d)
    } else {
        Err(fail(format!("no add-on {name:?} is installed")))
    }
}

fn set_enabled(name: &str, on: bool) -> Result<(), Fail> {
    let dir = installed_dir(name)?;
    if on {
        Addon::load(&dir).map_err(fail)?;
    }
    let mut names = enabled()?;
    names.retain(|n| n != name);
    if on {
        names.push(name.to_string());
    }
    save_enabled(&names)?;
    tell_daemon();
    Ok(())
}

fn remove(name: &str) -> Result<(), Fail> {
    let dir = installed_dir(name)?;
    let mut names = enabled()?;
    names.retain(|n| n != name);
    save_enabled(&names)?;
    std::fs::remove_dir_all(&dir).map_err(|e| fail(format!("{}: {e}", dir.display())))?;
    tell_daemon();
    Ok(())
}

/// B-ADDON-3: how long the CLI waits for `Reload`.
const RELOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// B-ADDON-3: a running daemon (none is started for this) reads the add-ons again
/// (`Agent1.Reload`).
fn tell_daemon() {
    use zbus::proxy::{CacheProperties, MethodFlags};
    let (tx, rx) = std::sync::mpsc::channel();
    let on_bus = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let connected = on_bus.clone();
    // zbus has no timeout for calls with flags; the thread ends with the process.
    std::thread::spawn(move || {
        // No session bus: no daemon either.
        let Ok(conn) = zbus::blocking::Connection::session() else {
            let _ = tx.send(Ok(None));
            return;
        };
        connected.store(true, std::sync::atomic::Ordering::Relaxed);
        let r = zbus::blocking::proxy::Builder::<zbus::blocking::Proxy>::new(&conn)
            .destination(BUS_NAME)
            .and_then(|b| b.path(AGENT_PATH))
            .and_then(|b| b.interface(AGENT_IFACE))
            .and_then(|b| b.cache_properties(CacheProperties::No).build())
            .and_then(|p| {
                p.call_with_flags::<_, _, ()>("Reload", MethodFlags::NoAutoStart.into(), &())
            });
        let _ = tx.send(r);
    });
    match rx.recv_timeout(RELOAD_TIMEOUT) {
        // On the bus, a name without an owner fails at once (NO_AUTO_START): the daemon
        // runs and is still rescanning; the call stays queued for it.
        Err(_) if on_bus.load(std::sync::atomic::Ordering::Relaxed) => {}
        Err(_) => eprintln!(
            "bentwire: the session bus did not answer within {} s; restart bentwired to \
             apply this",
            RELOAD_TIMEOUT.as_secs()
        ),
        Ok(Ok(_)) => {}
        Ok(Err(zbus::Error::MethodError(n, ..)))
            if matches!(
                n.as_str(),
                "org.freedesktop.DBus.Error.NameHasNoOwner"
                    | "org.freedesktop.DBus.Error.ServiceUnknown"
            ) => {}
        Ok(Err(e)) => {
            eprintln!("bentwire: bentwired did not reload ({e}); restart it to apply this")
        }
    }
}

/// B-ADDON-2: curl as in B-CLI-1, cut off after `max` bytes.
fn download(url: &str, to: &Path, max: u64) -> Result<(), Fail> {
    let mut child = curl(url)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| fail(format!("cannot run \"curl\": {e}")))?;
    let mut body = Capped {
        r: child.stdout.take().expect("piped"),
        left: max,
        what: "download is",
        limit: max,
    };
    let r = std::fs::File::create(to).and_then(|mut f| std::io::copy(&mut body, &mut f));
    if r.is_err() {
        let _ = child.kill();
    }
    let ok = child.wait().is_ok_and(|s| s.success());
    match r {
        Err(e) => Err(fail(format!("{url}: {e}"))),
        Ok(_) if ok => Ok(()),
        Ok(_) => Err(fail(format!("downloading {url} failed"))),
    }
}

fn install(src: &str, sha: Option<&str>) -> Result<(), Fail> {
    let sha = sha.map(str::to_ascii_lowercase);
    if sha
        .as_ref()
        .is_some_and(|h| h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(Fail::Usage("--sha256 needs 64 hex digits".into()));
    }
    let dir = addons_dir();
    std::fs::create_dir_all(&dir).map_err(|e| fail(format!("{}: {e}", dir.display())))?;
    let tmp = dir.join(format!(".install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir(&tmp).map_err(|e| fail(format!("{}: {e}", tmp.display())))?;
    let r = install_into(&tmp, &dir, src, sha.as_deref());
    let _ = std::fs::remove_dir_all(&tmp);
    let a = r?;
    outln!(
        "installed {} {} ({}) in {}",
        a.name,
        a.version,
        a.kind.as_str(),
        dir.join(&a.name).display()
    );
    if a.kind == Kind::Bridge {
        outln!(
            "a bridge runs only once enabled: bentwire addons enable {}",
            a.name
        );
    }
    tell_daemon();
    Ok(())
}

/// Unpack or copy `src` below `tmp`, check its manifest and rename it into `dir`, replacing
/// an older copy as a whole.
fn install_into(tmp: &Path, dir: &Path, src: &str, sha: Option<&str>) -> Result<Addon, Fail> {
    let stage = tmp.join("stage");
    let archive = if src.contains("://") {
        if !src.starts_with("https://") && !src.starts_with("file://") {
            return Err(fail(format!("{src}: only https:// (or file://) URLs")));
        }
        let f = tmp.join("download");
        eprintln!("bentwire: downloading {src}");
        download(src, &f, LIMITS.packed)?;
        Some(f)
    } else if Path::new(src).is_dir() {
        if sha.is_some() {
            return Err(Fail::Usage("--sha256 is for archives and URLs".into()));
        }
        if !Path::new(src).join(MANIFEST).is_file() {
            return Err(fail(format!("{src}: no {MANIFEST}")));
        }
        copy_tree(Path::new(src), &stage).map_err(|e| fail(format!("{src}: {e}")))?;
        None
    } else {
        Some(PathBuf::from(src))
    };
    if let Some(f) = &archive {
        if let Some(h) = sha
            && sha256_hex(f)? != h
        {
            return Err(fail(format!("{src}: SHA-256 does not match")));
        }
        extract(f, &stage, &LIMITS).map_err(|e| fail(format!("{src}: {e}")))?;
    }
    // The manifest is at the top, or in the only top-level folder of an archive.
    let root = if stage.join(MANIFEST).is_file() {
        stage
    } else {
        let top: Vec<PathBuf> = std::fs::read_dir(&stage)
            .map_err(|e| fail(format!("{}: {e}", stage.display())))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        match top.as_slice() {
            [d] if d.join(MANIFEST).is_file() => d.clone(),
            _ => return Err(fail(format!("{src}: no {MANIFEST}"))),
        }
    };
    let text = std::fs::read_to_string(root.join(MANIFEST))
        .map_err(|e| fail(format!("{src}: {MANIFEST}: {e}")))?;
    let a = Addon::parse(&text).map_err(|e| fail(format!("{src}: {e}")))?;
    let mut names = enabled()?;
    names.retain(|n| *n != a.name);
    // B-ADDON-3: an install never enables a bridge, also not one replacing an enabled
    // add-on; the list is saved before its code is in place.
    if a.kind == Kind::Bridge {
        save_enabled(&names)?;
    }
    let dest = dir.join(&a.name);
    let old = tmp.join("old");
    let had_old = dest.symlink_metadata().is_ok();
    if had_old {
        std::fs::rename(&dest, &old).map_err(|e| fail(format!("{}: {e}", dest.display())))?;
    }
    if let Err(e) = std::fs::rename(&root, &dest) {
        if had_old {
            let _ = std::fs::rename(&old, &dest);
        }
        return Err(fail(format!("{}: {e}", dest.display())));
    }
    if a.kind != Kind::Bridge {
        names.push(a.name.clone());
        save_enabled(&names)?;
    }
    Ok(a)
}

fn not_allowed(p: &Path) -> String {
    format!(
        "{}: links and special files are not allowed in an add-on",
        p.display()
    )
}

/// Copy a folder; links and special files are refused, modes become 0755 or 0644.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let rd = std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?;
    for e in rd {
        let p = e.map_err(|e| e.to_string())?.path();
        let t = to.join(p.file_name().unwrap_or_default());
        let m = p
            .symlink_metadata()
            .map_err(|e| format!("{}: {e}", p.display()))?;
        if m.is_dir() {
            copy_tree(&p, &t)?;
        } else if m.is_file() {
            use std::os::unix::fs::PermissionsExt;
            let mode = if m.permissions().mode() & 0o111 != 0 {
                0o755
            } else {
                0o644
            };
            std::fs::copy(&p, &t)
                .and_then(|_| std::fs::set_permissions(&t, std::fs::Permissions::from_mode(mode)))
                .map_err(|e| format!("{}: {e}", p.display()))?;
        } else {
            return Err(not_allowed(&p));
        }
    }
    Ok(())
}

/// B-ADDON-2: unpack a ZIP or gzip-compressed tar archive into the new folder `to`. Only
/// folders and regular files with relative paths that stay inside are allowed; the mode
/// becomes 0755 for executable files, else 0644, and the times are those of the unpacking.
/// `limits` are checked while unpacking, so no more than they allow is ever written.
fn extract(file: &Path, to: &Path, limits: &Limits) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    let mut f = std::fs::File::open(file).map_err(io)?;
    if f.metadata().map_err(io)?.len() > limits.packed {
        return Err(too_large("archive is", limits.packed));
    }
    let mut magic = [0u8; 4];
    let n = f.read(&mut magic).map_err(io)?;
    f.rewind().map_err(io)?;
    std::fs::create_dir(to).map_err(io)?;
    let unpacks = "archive unpacks to";
    let too_many = || too_many(limits.entries);
    // Every component of an entry's path counts: each may create a folder.
    let mut names = limits.entries;
    if magic[..n].starts_with(b"PK") {
        // Before the zip crate reads the whole central directory into memory.
        if zip_entries(&mut f).map_err(io)? > limits.entries as u64 {
            return Err(too_many());
        }
        let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
        if z.len() > limits.entries {
            return Err(too_many());
        }
        // One budget for all entries, counted in bytes actually unpacked.
        let mut left = limits.unpacked;
        for i in 0..z.len() {
            let e = z.by_index(i).map_err(|e| e.to_string())?;
            let name = PathBuf::from(e.name());
            // Only the type bits of a regular file or a folder, if any.
            let mode = e.unix_mode().unwrap_or(0);
            if !matches!(mode & 0o170000, 0 | 0o100000 | 0o040000) {
                return Err(not_allowed(&name));
            }
            let dir = e.is_dir();
            let mut r = Capped {
                r: e,
                left,
                what: unpacks,
                limit: limits.unpacked,
            };
            put(to, &name, dir, mode, &mut r, (&mut names, limits.entries))?;
            left = r.left;
        }
    } else if magic[..n].starts_with(&[0x1f, 0x8b]) {
        // The whole tar stream counts, headers included.
        let headers = Rc::new(Cell::new(TAR_HEADERS));
        let mut a = tar::Archive::new(Budget {
            r: Capped {
                r: flate2::read::GzDecoder::new(f),
                left: limits.unpacked,
                what: unpacks,
                limit: limits.unpacked,
            },
            left: headers.clone(),
        });
        for (i, e) in a.entries().map_err(io)?.enumerate() {
            if i >= limits.entries {
                return Err(too_many());
            }
            let mut e = e.map_err(io)?;
            headers.set(u64::MAX);
            let name = e.path().map_err(io)?.into_owned();
            let mode = e.header().mode().map_err(io)?;
            match e.header().entry_type() {
                tar::EntryType::Regular => {
                    put(to, &name, false, mode, &mut e, (&mut names, limits.entries))?
                }
                tar::EntryType::Directory => {
                    put(to, &name, true, mode, &mut e, (&mut names, limits.entries))?
                }
                // pax headers (GitHub tarballs carry a global one).
                tar::EntryType::XGlobalHeader | tar::EntryType::XHeader => {}
                _ => return Err(not_allowed(&name)),
            }
            // The rest (a folder's or pax header's data) is not part of the next headers.
            std::io::copy(&mut e, &mut std::io::sink()).map_err(io)?;
            headers.set(TAR_HEADERS);
        }
    } else {
        return Err("not a folder, a .zip or a .tar.gz archive".into());
    }
    Ok(())
}

/// The entry count of a zip file's end of central directory record (0 when there is none;
/// the zip crate then fails on its own). A Zip64 archive stores 0xFFFF here, which is above
/// any entry limit.
fn zip_entries(f: &mut std::fs::File) -> std::io::Result<u64> {
    use std::io::SeekFrom;
    let len = f.seek(SeekFrom::End(0))?;
    let back = len.min(22 + 0xFFFF);
    f.seek(SeekFrom::Start(len - back))?;
    let mut tail = Vec::with_capacity(back as usize);
    f.by_ref().take(back).read_to_end(&mut tail)?;
    f.rewind()?;
    let n = tail
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .and_then(|i| tail.get(i + 10..i + 12))
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    Ok(n.into())
}

fn too_many(limit: usize) -> String {
    format!("archive has more than {limit} entries (the limit for add-on archives)")
}

/// Write one archive entry below `to`. Nothing but folders and regular files is ever
/// created, so a checked relative path cannot leave `to`. Its path components are taken
/// from `names` (what is left, the limit) before anything is created.
fn put(
    to: &Path,
    name: &Path,
    dir: bool,
    mode: u32,
    r: &mut dyn Read,
    names: (&mut usize, usize),
) -> Result<(), String> {
    let mut rel = PathBuf::new();
    for c in name.components() {
        match c {
            Component::Normal(s) => rel.push(s),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "{}: absolute or leaves the add-on folder",
                    name.display()
                ));
            }
        }
    }
    let n = rel.components().count();
    if n > *names.0 {
        return Err(too_many(names.1));
    }
    *names.0 -= n;
    let p = to.join(rel);
    let err = |e: std::io::Error| format!("{}: {e}", name.display());
    if dir {
        return std::fs::create_dir_all(&p).map_err(err);
    }
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(err)?;
    }
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(if mode & 0o111 != 0 { 0o755 } else { 0o644 })
        .open(&p)
        .map_err(err)?;
    std::io::copy(r, &mut out).map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bw-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A tar.gz of (raw name, type, mode, data); names are written unchecked.
    fn tar_gz(path: &Path, entries: &[(&str, tar::EntryType, u32, &[u8])]) {
        let gz = flate2::write::GzEncoder::new(
            std::fs::File::create(path).unwrap(),
            flate2::Compression::fast(),
        );
        let mut b = tar::Builder::new(gz);
        for (name, t, mode, data) in entries {
            let mut h = tar::Header::new_gnu();
            h.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            h.set_entry_type(*t);
            h.set_mode(*mode);
            h.set_size(data.len() as u64);
            if *t == tar::EntryType::Symlink {
                h.set_link_name("/etc").unwrap();
            }
            h.set_cksum();
            b.append(&h, *data).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap();
    }

    fn zip_file(path: &Path, entries: &[(&str, Option<u32>, &[u8])], link: Option<&str>) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, mode, data) in entries {
            let mut o = zip::write::SimpleFileOptions::default();
            if let Some(m) = mode {
                o = o.unix_permissions(*m);
            }
            z.start_file(*name, o).unwrap();
            z.write_all(data).unwrap();
        }
        if let Some(l) = link {
            z.add_symlink(l, "/etc", zip::write::SimpleFileOptions::default())
                .unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn b_addon_2_extract_tar_gz_and_zip() {
        use tar::EntryType::{Directory, Regular};
        let d = tempdir("extract");
        let t = d.join("a.tar.gz");
        tar_gz(
            &t,
            &[
                (
                    "pax_global_header",
                    tar::EntryType::XGlobalHeader,
                    0o644,
                    b"",
                ),
                ("top/", Directory, 0o755, b""),
                ("top/addon.toml", Regular, 0o600, b"x"),
                ("./top/run.sh", Regular, 0o700, b"#!/bin/sh\n"),
            ],
        );
        extract(&t, &d.join("t"), &LIMITS).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&d.join("t/top/addon.toml")), 0o644);
        assert_eq!(mode(&d.join("t/top/run.sh")), 0o755);
        let z = d.join("a.zip");
        zip_file(
            &z,
            &[("addon.toml", None, b"y"), ("bin/run", Some(0o755), b"z")],
            None,
        );
        extract(&z, &d.join("z"), &LIMITS).unwrap();
        assert_eq!(std::fs::read(d.join("z/addon.toml")).unwrap(), b"y");
        assert_eq!(mode(&d.join("z/bin/run")), 0o755);
        let other = d.join("x.txt");
        std::fs::write(&other, "hello").unwrap();
        let e = extract(&other, &d.join("o"), &LIMITS).unwrap_err();
        assert!(e.contains("not a folder"), "{e}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// B-ADDON-2: `..`, absolute paths and links are refused, and nothing is written outside.
    #[test]
    fn b_addon_2_extract_rejects_escapes() {
        use tar::EntryType::{Link, Regular, Symlink};
        let d = tempdir("escape");
        let cases_tar: [(&str, tar::EntryType); 5] = [
            ("../evil", Regular),
            ("a/../../evil", Regular),
            ("/tmp/bw-evil-abs", Regular),
            ("link", Symlink),
            ("hard", Link),
        ];
        for (i, (name, t)) in cases_tar.iter().enumerate() {
            let f = d.join(format!("{i}.tar.gz"));
            tar_gz(&f, &[(name, *t, 0o644, b"x")]);
            let e = extract(&f, &d.join(format!("out{i}")), &LIMITS).unwrap_err();
            assert!(
                e.contains("leaves the add-on folder") || e.contains("links"),
                "{name}: {e}"
            );
        }
        for (i, name) in ["../evil", "/tmp/bw-evil-abs"].iter().enumerate() {
            let f = d.join(format!("{i}.zip"));
            zip_file(&f, &[(name, None, b"x")], None);
            let e = extract(&f, &d.join(format!("zout{i}")), &LIMITS).unwrap_err();
            assert!(e.contains("leaves the add-on folder"), "{name}: {e}");
        }
        let f = d.join("link.zip");
        zip_file(&f, &[("addon.toml", None, b"x")], Some("etc"));
        let e = extract(&f, &d.join("zlink"), &LIMITS).unwrap_err();
        assert!(e.contains("links"), "{e}");
        assert!(!d.join("evil").exists() && !Path::new("/tmp/bw-evil-abs").exists());
        // Folders: a link is refused too.
        let src = d.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::os::unix::fs::symlink("/etc", src.join("etc")).unwrap();
        let e = copy_tree(&src, &d.join("copy")).unwrap_err();
        assert!(e.contains("links"), "{e}");
        // Folder modes become 0755 or 0644 like archive modes.
        use std::os::unix::fs::PermissionsExt;
        let plain = d.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        for (f, m) in [("run", 0o777), ("data", 0o666)] {
            std::fs::write(plain.join(f), "x").unwrap();
            std::fs::set_permissions(plain.join(f), std::fs::Permissions::from_mode(m)).unwrap();
        }
        copy_tree(&plain, &d.join("pcopy")).unwrap();
        let mode = |f: &str| {
            let m = std::fs::metadata(d.join("pcopy").join(f)).unwrap();
            m.permissions().mode() & 0o7777
        };
        assert_eq!((mode("run"), mode("data")), (0o755, 0o644));
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// The bytes of all files below `d`.
    fn disk(d: &Path) -> u64 {
        std::fs::read_dir(d).map_or(0, |rd| {
            rd.map(|e| {
                let p = e.unwrap().path();
                if p.is_dir() {
                    disk(&p)
                } else {
                    p.metadata().unwrap().len()
                }
            })
            .sum()
        })
    }

    /// B-ADDON-2: GNU long names and pax extensions, which the tar crate holds in memory,
    /// are refused above 1 MiB per entry; a usual long name and a pax header of a few
    /// hundred KiB with a long path still unpack.
    #[test]
    fn b_addon_2_tar_extension_headers_are_capped() {
        use tar::EntryType::{GNULongName, Regular, XHeader};
        let d = tempdir("headers");
        let long = format!("top/{}/addon.toml", "n".repeat(150));
        let f = d.join("long.tar.gz");
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(
            std::fs::File::create(&f).unwrap(),
            flate2::Compression::fast(),
        ));
        let mut h = tar::Header::new_gnu();
        h.set_mode(0o644);
        h.set_size(1);
        b.append_data(&mut h, &long, &b"x"[..]).unwrap();
        b.into_inner().unwrap().finish().unwrap();
        extract(&f, &d.join("long"), &LIMITS).unwrap();
        assert_eq!(std::fs::read(d.join("long").join(&long)).unwrap(), b"x");
        let path = format!(
            "top/{}addon.toml",
            format!("{}/", "p".repeat(200)).repeat(10)
        );
        let record = |k: &str, v: &str| {
            let body = format!(" {k}={v}\n");
            let n = (body.len()..).find(|n| n.to_string().len() + body.len() == *n);
            format!("{}{body}", n.unwrap())
        };
        let pax = record("comment", &"c".repeat(300 << 10)) + &record("path", &path);
        let f = d.join("pax.tar.gz");
        tar_gz(
            &f,
            &[
                ("././@PaxHeader", XHeader, 0o644, pax.as_bytes()),
                ("x", Regular, 0o644, b"x"),
            ],
        );
        extract(&f, &d.join("pax"), &LIMITS).unwrap();
        assert_eq!(std::fs::read(d.join("pax").join(&path)).unwrap(), b"x");
        let big = vec![b'a'; 2 << 20];
        for (i, t) in [GNULongName, XHeader].into_iter().enumerate() {
            let f = d.join(format!("big{i}.tar.gz"));
            tar_gz(
                &f,
                &[
                    ("././@LongLink", t, 0o644, &big),
                    ("x", Regular, 0o644, b"x"),
                ],
            );
            let e = extract(&f, &d.join(format!("big{i}")), &LIMITS).unwrap_err();
            assert!(
                e.contains(
                    "an entry's tar headers are more than 1 MiB (the limit for add-on archives)"
                ),
                "{t:?}: {e}"
            );
        }
    }

    /// B-ADDON-2: packed size, unpacked size (zip bombs: small packed, large unpacked; one
    /// budget for all entries) and entry count are limited while unpacking, and downloads
    /// while downloading; nothing over the limit reaches the disk.
    #[test]
    fn b_addon_2_archive_limits() {
        use tar::EntryType::Regular;
        let d = tempdir("limits");
        let small = Limits {
            packed: 1 << 20,
            unpacked: 1 << 20,
            entries: 10,
        };
        let zeros = vec![0u8; 4 << 20];
        let half = vec![0u8; 600 << 10];
        let z = d.join("bomb.zip");
        zip_file(
            &z,
            &[("addon.toml", None, b"x"), ("big", None, &zeros)],
            None,
        );
        let t = d.join("bomb.tar.gz");
        tar_gz(
            &t,
            &[
                ("addon.toml", Regular, 0o644, b"x"),
                ("big", Regular, 0o644, &zeros),
            ],
        );
        let two = d.join("two.zip");
        zip_file(&two, &[("a", None, &half), ("b", None, &half)], None);
        for (i, f) in [&z, &t, &two].into_iter().enumerate() {
            assert!(
                f.metadata().unwrap().len() < 64 << 10,
                "{f:?} is small packed"
            );
            let out = d.join(format!("bomb{i}"));
            let e = extract(f, &out, &small).unwrap_err();
            assert!(
                e.contains("archive unpacks to more than 1 MiB (the limit for add-on archives)"),
                "{f:?}: {e}"
            );
            assert!(disk(&out) <= 1 << 20, "{f:?}: {} bytes written", disk(&out));
            extract(f, &d.join(format!("ok{i}")), &LIMITS).unwrap();
        }
        // Entries.
        let names: Vec<String> = (0..11).map(|i| format!("f{i}")).collect();
        let many_zip: Vec<(&str, Option<u32>, &[u8])> = names
            .iter()
            .map(|n| (n.as_str(), None, &b"x"[..]))
            .collect();
        let many_tar: Vec<(&str, tar::EntryType, u32, &[u8])> = names
            .iter()
            .map(|n| (n.as_str(), Regular, 0o644, &b"x"[..]))
            .collect();
        let mz = d.join("many.zip");
        zip_file(&mz, &many_zip, None);
        assert_eq!(
            zip_entries(&mut std::fs::File::open(&mz).unwrap()).unwrap(),
            11
        );
        // An end record claiming 60000 entries is refused before the central directory is
        // read (the zip crate would reserve memory for all of them).
        let lying = d.join("lying.zip");
        let mut eocd = b"PK\x05\x06\0\0\0\0".to_vec();
        eocd.extend([0x60, 0xEA, 0x60, 0xEA, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        std::fs::write(&lying, eocd).unwrap();
        let e = extract(&lying, &d.join("lying"), &LIMITS).unwrap_err();
        assert!(e.contains("archive has more than 10000 entries"), "{e}");
        let mt = d.join("many.tar.gz");
        tar_gz(&mt, &many_tar);
        for (i, f) in [&mz, &mt].into_iter().enumerate() {
            let e = extract(f, &d.join(format!("many{i}")), &small).unwrap_err();
            assert!(e.contains("archive has more than 10 entries"), "{f:?}: {e}");
            let ten = Limits {
                entries: 11,
                ..small
            };
            extract(f, &d.join(format!("eleven{i}")), &ten).unwrap();
        }
        // Every folder in a path counts: one entry 11 levels deep creates nothing.
        let deep = d.join("deep.zip");
        zip_file(&deep, &[("a/a/a/a/a/a/a/a/a/a/f", None, b"x")], None);
        let e = extract(&deep, &d.join("deep"), &small).unwrap_err();
        assert!(e.contains("archive has more than 10 entries"), "{e}");
        assert_eq!(std::fs::read_dir(d.join("deep")).unwrap().count(), 0);
        // Packed size.
        let tiny = Limits {
            packed: 100,
            ..small
        };
        let e = extract(&mt, &d.join("packed"), &tiny).unwrap_err();
        assert!(e.contains("archive is more than 100 bytes"), "{e}");
        // A download stops at its limit.
        let big = d.join("big.bin");
        std::fs::write(&big, &zeros).unwrap();
        let to = d.join("dl");
        let url = format!("file://{}", big.display());
        let Err(Fail::Exit(1, Some(e))) = download(&url, &to, 1 << 20) else {
            panic!("download over the limit");
        };
        assert!(e.contains("download is more than 1 MiB"), "{e}");
        assert!(to.metadata().unwrap().len() <= 1 << 20);
        assert!(download(&url, &to, 4 << 20).is_ok());
        assert_eq!(to.metadata().unwrap().len(), 4 << 20);
        std::fs::remove_dir_all(&d).unwrap();
    }
}
