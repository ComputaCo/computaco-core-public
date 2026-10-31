//! Small host helpers: time, hashing files, walking a directory, running commands.
use cw_fidelity::hash::sha256_hex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub type Result<T> = std::result::Result<T, String>;

pub fn err<T>(m: impl Into<String>) -> Result<T> {
    Err(m.into())
}

/// ISO 8601 UTC to the second, without a time-zone library: civil-from-days (Howard
/// Hinnant's algorithm).
pub fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

pub fn sha256_path(p: &Path) -> Option<String> {
    std::fs::read(p).ok().map(|b| sha256_hex(&b))
}

/// Every regular file under `root`, by path relative to it, with hash and size.
/// `ignore` are `*` globs over the relative path.
pub fn walk(root: &Path, ignore: &[String]) -> BTreeMap<String, (String, u64)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if ignore.iter().any(|g| {
                cw_fidelity::text::glob_matches(g, &rel)
                    || cw_fidelity::text::glob_matches(g, &format!("{rel}/"))
            }) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                if let Some(h) = sha256_path(&path) {
                    out.insert(rel, (h, meta.len()));
                }
            }
        }
    }
    out
}

/// Run a command to completion and return stdout; a non-zero exit is an error with
/// stderr.
pub fn run(
    cmd: &str,
    args: &[&str],
    env: &BTreeMap<String, String>,
    timeout: Duration,
) -> Result<String> {
    let mut c = Command::new(cmd);
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        c.env(k, v);
    }
    let mut child = c.spawn().map_err(|e| format!("{cmd}: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return err(format!(
                    "{cmd} {}: timed out after {:?}",
                    args.join(" "),
                    timeout
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return err(format!("{cmd}: {e}")),
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if !out.status.success() {
        return err(format!(
            "{cmd} {}: exit {:?}: {}",
            args.join(" "),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

pub fn read_json<T: serde::de::DeserializeOwned>(p: &Path) -> Result<T> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
}
pub fn write_json<T: serde::Serialize>(p: &Path, v: &T) -> Result<()> {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    std::fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()))
}

/// Progress line on stderr, one JSON object per line so a caller can parse it.
pub fn progress(event: &str, fields: serde_json::Value) {
    let mut v = fields;
    if let serde_json::Value::Object(m) = &mut v {
        m.insert("event".into(), serde_json::Value::String(event.to_owned()));
    }
    eprintln!("{v}");
}
