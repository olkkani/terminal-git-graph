use terminal_git_graph::gfx::detect::*;
use terminal_git_graph::gfx::CellPx;

const KITTY_OK: &[u8] = b"\x1b_Gi=31;OK\x1b\\";
const CELL: &[u8] = b"\x1b[6;20;10t";
const DA1: &[u8] = b"\x1b[?62;22c";

#[test]
fn parses_kitty_ok_then_cell_then_da1() {
    let buf = [KITTY_OK, CELL, DA1].concat();
    assert_eq!(
        parse_probe_reply(&buf),
        ProbeReply {
            kitty_ok: true,
            cell: Some(CellPx { w: 10, h: 20 }),
            done: true,
        }
    );
}

#[test]
fn kitty_error_reply_is_not_ok() {
    let buf = [
        b"\x1b_Gi=31;ENOTSUPPORTED:unsupported\x1b\\".as_slice(),
        DA1,
    ]
    .concat();
    let reply = parse_probe_reply(&buf);
    assert!(!reply.kitty_ok);
    assert!(reply.done);
}

#[test]
fn only_da1_reply_means_unsupported() {
    assert_eq!(
        parse_probe_reply(DA1),
        ProbeReply {
            kitty_ok: false,
            cell: None,
            done: true,
        }
    );
}

#[test]
fn partial_reply_is_not_done() {
    let buf = [KITTY_OK, b"\x1b[6;20".as_slice()].concat();
    let reply = parse_probe_reply(&buf);
    assert!(reply.kitty_ok);
    assert_eq!(reply.cell, None);
    assert!(!reply.done);

    // DA1 cut off before its final byte is not done either.
    assert!(!parse_probe_reply(b"\x1b[?62;22").done);
}

#[test]
fn cell_size_reply_parsed() {
    assert_eq!(parse_probe_reply(CELL).cell, Some(CellPx { w: 10, h: 20 }));
}

#[test]
fn window_pixels_zero_is_none() {
    assert_eq!(cell_px_from_window((0, 0), (160, 40)), None);
    assert_eq!(cell_px_from_window((1600, 0), (160, 40)), None);
    assert_eq!(cell_px_from_window((1600, 800), (0, 40)), None);
}

#[test]
fn cell_px_division() {
    assert_eq!(
        cell_px_from_window((1600, 800), (160, 40)),
        Some(CellPx { w: 10, h: 20 })
    );
}

#[test]
fn env_blocks_tmux_zellij_and_dumb() {
    assert!(env_blocks_graphics(true, false, Some("xterm-kitty")));
    assert!(env_blocks_graphics(false, true, Some("xterm-kitty")));
    assert!(env_blocks_graphics(false, false, Some("dumb")));
    assert!(env_blocks_graphics(false, false, Some("linux")));
    assert!(!env_blocks_graphics(false, false, Some("xterm-ghostty")));
    assert!(!env_blocks_graphics(false, false, None));
}

#[test]
fn probe_request_ends_with_da1() {
    let req = probe_request();
    assert!(req.starts_with(b"\x1b_Gi=31,"));
    assert!(req.windows(5).any(|w| w == b"\x1b[16t"));
    assert!(req.ends_with(b"\x1b[c"));
}

use std::time::{Duration, Instant};

/// Fake reader idle step: sleeps for the offered wait, cut short at `until`.
fn idle_wait(wait: Duration, until: Option<Instant>) {
    let nap = match until {
        Some(t) => wait.min(t.saturating_duration_since(Instant::now())),
        None => wait,
    };
    std::thread::sleep(nap);
}

#[test]
fn late_terminator_within_grace_is_consumed() {
    let start = Instant::now();
    let timeout = Duration::from_millis(20);
    let arrives = start + Duration::from_millis(80);
    let mut sent = false;
    let reply = collect_probe_reply(start + timeout, PROBE_GRACE, |wait, buf| {
        if !sent && Instant::now() >= arrives {
            buf.extend_from_slice(&[KITTY_OK, CELL, DA1].concat());
            sent = true;
        } else {
            idle_wait(wait, Some(arrives));
        }
        true
    });
    assert!(reply.done);
    assert!(reply.kitty_ok);
    assert_eq!(reply.cell, Some(CellPx { w: 10, h: 20 }));
    assert!(start.elapsed() < timeout + PROBE_GRACE);
}

#[test]
fn silent_terminal_ends_after_timeout_plus_grace() {
    let start = Instant::now();
    let timeout = Duration::from_millis(30);
    let reply = collect_probe_reply(start + timeout, PROBE_GRACE, |wait, _| {
        idle_wait(wait, None);
        true
    });
    let elapsed = start.elapsed();
    assert!(!reply.done);
    assert!(!reply.kitty_ok);
    assert!(elapsed >= timeout + PROBE_GRACE);
    assert!(elapsed < timeout + PROBE_GRACE + Duration::from_millis(100));
}

#[test]
fn replies_split_across_reads_are_reassembled() {
    let all = [KITTY_OK, CELL, DA1].concat();
    let mut chunks = vec![
        &all[..3],
        &all[3..12],
        &all[12..17],
        &all[17..all.len() - 1],
    ];
    chunks.push(&all[all.len() - 1..]);
    let mut chunks = chunks.into_iter();
    let mut reads = 0;
    let reply = collect_probe_reply(
        Instant::now() + Duration::from_secs(5),
        PROBE_GRACE,
        |_, buf| {
            reads += 1;
            buf.extend_from_slice(chunks.next().expect("read after DA1"));
            true
        },
    );
    assert_eq!(reads, 5);
    assert_eq!(
        reply,
        ProbeReply {
            kitty_ok: true,
            cell: Some(CellPx { w: 10, h: 20 }),
            done: true,
        }
    );
}

#[test]
fn closed_input_stops_collecting() {
    let reply = collect_probe_reply(
        Instant::now() + Duration::from_secs(5),
        PROBE_GRACE,
        |_, _| false,
    );
    assert!(!reply.done);
}
