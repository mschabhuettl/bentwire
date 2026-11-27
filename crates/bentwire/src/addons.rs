//! `bentwire addons` (spec B-ADDON-2, B-ADDON-3): add-ons in `$XDG_DATA_HOME/bentwire/addons`,
//! the enabled list in `$XDG_CONFIG_HOME/bentwire/addons.toml`.

use crate::{BUS_NAME, Fail, Opts, download, fail, sha256_hex, xdg_dir};
use bentwire_character::addon::{self, Addon, Kind, MANIFEST};
use std::io::{Read, Seek};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

const USAGE: &str =
    "expected list, install <folder|archive|url> [--sha256 HEX], enable, disable or remove <name>";

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

/// A running daemon (none is started for this) gets SIGHUP and reads the add-ons again.
fn tell_daemon() {
    let Ok(conn) = zbus::blocking::Connection::session() else {
        return;
    };
    let pid = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "GetConnectionUnixProcessID",
            &(BUS_NAME,),
        )
        .ok()
        .and_then(|m| m.body().deserialize::<u32>().ok());
    if let Some(pid) = pid {
        // SAFETY: plain kill(2).
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGHUP);
        }
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
        download(src, &f)?;
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
        extract(f, &stage).map_err(|e| fail(format!("{src}: {e}")))?;
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
fn extract(file: &Path, to: &Path) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    let mut f = std::fs::File::open(file).map_err(io)?;
    let mut magic = [0u8; 4];
    let n = f.read(&mut magic).map_err(io)?;
    f.rewind().map_err(io)?;
    std::fs::create_dir(to).map_err(io)?;
    if magic[..n].starts_with(b"PK") {
        let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
        for i in 0..z.len() {
            let mut e = z.by_index(i).map_err(|e| e.to_string())?;
            let name = PathBuf::from(e.name());
            // Only the type bits of a regular file or a folder, if any.
            let mode = e.unix_mode().unwrap_or(0);
            if !matches!(mode & 0o170000, 0 | 0o100000 | 0o040000) {
                return Err(not_allowed(&name));
            }
            put(to, &name, e.is_dir(), mode, &mut e)?;
        }
    } else if magic[..n].starts_with(&[0x1f, 0x8b]) {
        let mut a = tar::Archive::new(flate2::read::GzDecoder::new(f));
        for e in a.entries().map_err(io)? {
            let mut e = e.map_err(io)?;
            let name = e.path().map_err(io)?.into_owned();
            let mode = e.header().mode().map_err(io)?;
            match e.header().entry_type() {
                tar::EntryType::Regular => put(to, &name, false, mode, &mut e)?,
                tar::EntryType::Directory => put(to, &name, true, mode, &mut e)?,
                // pax headers (GitHub tarballs carry a global one).
                tar::EntryType::XGlobalHeader | tar::EntryType::XHeader => {}
                _ => return Err(not_allowed(&name)),
            }
        }
    } else {
        return Err("not a folder, a .zip or a .tar.gz archive".into());
    }
    Ok(())
}

/// Write one archive entry below `to`. Nothing but folders and regular files is ever
/// created, so a checked relative path cannot leave `to`.
fn put(to: &Path, name: &Path, dir: bool, mode: u32, r: &mut dyn Read) -> Result<(), String> {
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
        extract(&t, &d.join("t")).unwrap();
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
        extract(&z, &d.join("z")).unwrap();
        assert_eq!(std::fs::read(d.join("z/addon.toml")).unwrap(), b"y");
        assert_eq!(mode(&d.join("z/bin/run")), 0o755);
        let other = d.join("x.txt");
        std::fs::write(&other, "hello").unwrap();
        let e = extract(&other, &d.join("o")).unwrap_err();
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
            let e = extract(&f, &d.join(format!("out{i}"))).unwrap_err();
            assert!(
                e.contains("leaves the add-on folder") || e.contains("links"),
                "{name}: {e}"
            );
        }
        for (i, name) in ["../evil", "/tmp/bw-evil-abs"].iter().enumerate() {
            let f = d.join(format!("{i}.zip"));
            zip_file(&f, &[(name, None, b"x")], None);
            let e = extract(&f, &d.join(format!("zout{i}"))).unwrap_err();
            assert!(e.contains("leaves the add-on folder"), "{name}: {e}");
        }
        let f = d.join("link.zip");
        zip_file(&f, &[("addon.toml", None, b"x")], Some("etc"));
        let e = extract(&f, &d.join("zlink")).unwrap_err();
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
}
