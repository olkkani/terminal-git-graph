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
fn env_blocks_tmux_and_dumb() {
    assert!(env_blocks_graphics(true, Some("xterm-kitty")));
    assert!(env_blocks_graphics(false, Some("dumb")));
    assert!(env_blocks_graphics(false, Some("linux")));
    assert!(!env_blocks_graphics(false, Some("xterm-ghostty")));
    assert!(!env_blocks_graphics(false, None));
}

#[test]
fn probe_request_ends_with_da1() {
    let req = probe_request();
    assert!(req.starts_with(b"\x1b_Gi=31,"));
    assert!(req.windows(5).any(|w| w == b"\x1b[16t"));
    assert!(req.ends_with(b"\x1b[c"));
}
