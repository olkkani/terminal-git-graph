//! Copies text to the system clipboard: `pbcopy` on macOS, else (or when it fails) the
//! OSC 52 terminal sequence.

use std::io::{self, Write};
use std::process::{Command, Stdio};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// How the text was handed to the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyVia {
    Pbcopy,
    /// Written to the terminal; whether the terminal accepted it cannot be known.
    Osc52,
}

/// The OSC 52 sequence that asks the terminal to put `text` on the clipboard.
pub fn osc52_sequence(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", STANDARD.encode(text))
}

/// Copies `text`, trying `pbcopy` first on macOS. Errors only when every way failed.
pub fn copy(text: &str) -> Result<CopyVia, io::Error> {
    if cfg!(target_os = "macos") && pbcopy(text).is_ok() {
        return Ok(CopyVia::Pbcopy);
    }
    let mut out = io::stdout();
    out.write_all(osc52_sequence(text).as_bytes())?;
    out.flush()?;
    Ok(CopyVia::Osc52)
}

fn pbcopy(text: &str) -> io::Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(text.as_bytes())?;
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("pbcopy failed: {status}")))
    }
}
