//! Turns a git remote URL into the address of its web page, and opens it.

use std::io;
use std::process::{Command, Stdio};

/// The web page of a remote: `git@host:o/r.git`, `ssh://`, `git://`, `http(s)://`
/// forms become `https://host/o/r` (http stays http). Credentials, the SSH port and
/// `.git` are dropped. Local paths and `file://` give `None`.
pub fn to_https(remote: &str) -> Option<String> {
    let remote = remote.trim();
    if remote.contains('\\') || remote.chars().any(char::is_whitespace) {
        return None;
    }
    let (scheme, host, path) = if let Some((scheme, rest)) = remote.split_once("://") {
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        match scheme {
            "https" | "http" => (scheme, host, path),
            // The SSH port has nothing to do with the web server's.
            "ssh" | "git" | "git+ssh" => ("https", host.split(':').next()?, path),
            _ => return None,
        }
    } else {
        // scp-like `[user@]host:path`; a slash before the colon means a local path.
        let (authority, path) = remote.split_once(':')?;
        if authority.contains('/') {
            return None;
        }
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        ("https", host, path)
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{host}/{path}"))
}

/// Opens `url` in the default browser: `open` on macOS, `xdg-open` elsewhere.
pub fn open_in_browser(url: &str) -> io::Result<()> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let status = Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{program} exited with {status}")))
    }
}
