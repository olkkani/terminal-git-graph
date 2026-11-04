//! Kitty graphics capability detection.
//!
//! The probe writes a Kitty graphics query, a cell-size query (`CSI 16 t`) and a
//! primary device attributes request (`CSI c`) to the terminal. Every terminal
//! answers DA1, so its reply marks the end of the response stream.

const ESC: u8 = 0x1b;
const PROBE_ID: &str = "31";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellPx {
    pub w: u16,
    pub h: u16,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProbeReply {
    pub kitty_ok: bool,
    pub cell: Option<CellPx>,
    pub done: bool,
}

/// Kitty query (id 31, 1x1 RGB pixel) + cell size query + DA1.
pub fn probe_request() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\");
    out.extend_from_slice(b"\x1b[16t");
    out.extend_from_slice(b"\x1b[c");
    out
}

/// Scans terminal replies. Incomplete trailing sequences are ignored, so a
/// partial buffer simply reports what has fully arrived so far.
pub fn parse_probe_reply(buf: &[u8]) -> ProbeReply {
    let mut reply = ProbeReply {
        kitty_ok: false,
        cell: None,
        done: false,
    };
    let mut i = 0;
    while i < buf.len() {
        if buf[i] != ESC || i + 1 >= buf.len() {
            i += 1;
            continue;
        }
        match buf[i + 1] {
            b'_' => {
                let start = i + 2;
                let Some(len) = buf[start..].windows(2).position(|w| w == b"\x1b\\") else {
                    break;
                };
                if is_kitty_ok(&buf[start..start + len]) {
                    reply.kitty_ok = true;
                }
                i = start + len + 2;
            }
            b'[' => {
                let start = i + 2;
                let Some(len) = buf[start..].iter().position(|b| (0x40..=0x7e).contains(b)) else {
                    break;
                };
                let params = &buf[start..start + len];
                match buf[start + len] {
                    b't' => {
                        if let Some(cell) = parse_cell_size(params) {
                            reply.cell = Some(cell);
                        }
                    }
                    b'c' if params.first() == Some(&b'?') => reply.done = true,
                    _ => {}
                }
                i = start + len + 1;
            }
            _ => i += 1,
        }
    }
    reply
}

/// `G<keys>;<message>` where keys include `i=31` and the message is `OK`.
fn is_kitty_ok(payload: &[u8]) -> bool {
    let Some(rest) = payload.strip_prefix(b"G") else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return false;
    };
    let Some((keys, message)) = text.split_once(';') else {
        return false;
    };
    message == "OK" && keys.split(',').any(|kv| kv == format!("i={PROBE_ID}"))
}

/// `6;<height>;<width>` from a `CSI … t` reply.
fn parse_cell_size(params: &[u8]) -> Option<CellPx> {
    let text = std::str::from_utf8(params).ok()?;
    let mut parts = text.split(';');
    if parts.next()? != "6" {
        return None;
    }
    let h: u16 = parts.next()?.parse().ok()?;
    let w: u16 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || w == 0 || h == 0 {
        return None;
    }
    Some(CellPx { w, h })
}

/// Cell size from window pixels / cells, `None` when any value is zero.
pub fn cell_px_from_window(pixels: (u16, u16), cells: (u16, u16)) -> Option<CellPx> {
    let (pw, ph) = pixels;
    let (cols, rows) = cells;
    if pw == 0 || ph == 0 || cols == 0 || rows == 0 {
        return None;
    }
    let w = pw / cols;
    let h = ph / rows;
    if w == 0 || h == 0 {
        return None;
    }
    Some(CellPx { w, h })
}

/// Environments where the probe is skipped entirely.
pub fn env_blocks_graphics(tmux: bool, zellij: bool, term: Option<&str>) -> bool {
    tmux || zellij || matches!(term, Some("dumb") | Some("linux"))
}

/// Extra wait after the probe deadline for a terminator that is still in
/// flight, so late replies are consumed instead of leaking as key presses.
pub const PROBE_GRACE: std::time::Duration = std::time::Duration::from_millis(150);

/// Collects probe replies until DA1 arrives or `deadline + grace` passes.
/// `read(wait, buf)` waits at most `wait` for input, appends whatever bytes
/// are available to `buf` (possibly none) and returns `false` when the input
/// is closed or failed.
pub fn collect_probe_reply<F>(
    deadline: std::time::Instant,
    grace: std::time::Duration,
    mut read: F,
) -> ProbeReply
where
    F: FnMut(std::time::Duration, &mut Vec<u8>) -> bool,
{
    let end = deadline + grace;
    let mut buf = Vec::new();
    loop {
        let reply = parse_probe_reply(&buf);
        if reply.done {
            return reply;
        }
        let remaining = end.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() || !read(remaining, &mut buf) {
            return parse_probe_reply(&buf);
        }
    }
}

/// Queries the controlling terminal. Returns the cell size only when the Kitty
/// query succeeded and a cell size is known. The terminal settings are
/// restored on every exit path and all bytes read are consumed here; if no
/// DA1 arrives in time, the input queue is flushed before restoring.
#[cfg(unix)]
pub fn probe_terminal(timeout: std::time::Duration) -> Option<CellPx> {
    use std::io::{ErrorKind, Read, Write};
    use std::os::fd::{AsRawFd, RawFd};
    use std::time::Instant;

    struct RestoreTermios {
        fd: RawFd,
        orig: libc::termios,
    }
    impl Drop for RestoreTermios {
        fn drop(&mut self) {
            // SAFETY: fd is open for the guard's lifetime; orig came from tcgetattr.
            unsafe {
                libc::tcsetattr(self.fd, libc::TCSANOW, &self.orig);
            }
        }
    }

    let deadline = Instant::now() + timeout;
    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let fd = tty.as_raw_fd();

    // SAFETY: termios is plain data; tcgetattr fully initializes it on success.
    let mut orig: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut orig) } != 0 {
        return None;
    }
    // Declared after `tty`, so it drops (restores) before the fd is closed.
    let _guard = RestoreTermios { fd, orig };
    let mut raw = orig;
    // SAFETY: raw is a valid termios; fd is an open tty.
    if unsafe {
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(fd, libc::TCSANOW, &raw)
    } != 0
    {
        return None;
    }

    tty.write_all(&probe_request()).ok()?;
    tty.flush().ok()?;

    let mut chunk = [0u8; 256];
    let reply = collect_probe_reply(deadline, PROBE_GRACE, |wait, buf| {
        let ms = wait
            .as_micros()
            .div_ceil(1000)
            .min(libc::c_int::MAX as u128) as libc::c_int;
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: pfd is a valid pollfd array of length 1.
        let n = unsafe { libc::poll(&mut pfd, 1, ms) };
        if n < 0 {
            return std::io::Error::last_os_error().kind() == ErrorKind::Interrupted;
        }
        if n == 0 {
            return true;
        }
        if pfd.revents & libc::POLLIN == 0 {
            return false;
        }
        match tty.read(&mut chunk) {
            Ok(0) => false,
            Ok(len) => {
                buf.extend_from_slice(&chunk[..len]);
                true
            }
            Err(e) => e.kind() == ErrorKind::Interrupted,
        }
    });
    if !reply.done {
        // Late replies would otherwise reach the app as key presses.
        // SAFETY: fd is an open tty.
        unsafe {
            libc::tcflush(fd, libc::TCIFLUSH);
        }
    }

    if !reply.kitty_ok {
        return None;
    }
    reply.cell.or_else(|| {
        let size = crossterm::terminal::window_size().ok()?;
        cell_px_from_window((size.width, size.height), (size.columns, size.rows))
    })
}
