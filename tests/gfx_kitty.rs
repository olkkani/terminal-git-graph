use base64::{engine::general_purpose::STANDARD, Engine};
use terminal_git_graph::gfx::kitty::*;
use terminal_git_graph::gfx::GfxError;

#[test]
fn transmit_small_image_single_chunk() {
    let rgba = [255u8, 0, 0, 255, 0, 255, 0, 128];
    let out = transmit_rgba(ImageId(7), 2, 1, &rgba).unwrap();
    let expected = format!("\x1b_Ga=t,f=32,s=2,v=1,i=7,q=2,m=0;{}\x1b\\", STANDARD.encode(rgba));
    assert_eq!(out, expected.as_bytes());
}

#[test]
fn transmit_large_image_is_chunked_at_4096() {
    let (w, h) = (64u32, 64u32);
    let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i % 251) as u8).collect();
    let out = String::from_utf8(transmit_rgba(ImageId(1), w, h, &rgba).unwrap()).unwrap();
    let chunks: Vec<&str> = out.split("\x1b\\").filter(|s| !s.is_empty()).collect();
    assert!(chunks.len() > 1);
    let mut payload = String::new();
    for (i, c) in chunks.iter().enumerate() {
        let body = c.strip_prefix("\x1b_G").unwrap();
        let (keys, data) = body.split_once(';').unwrap();
        assert!(data.len() <= 4096);
        let last = i == chunks.len() - 1;
        assert!(keys.ends_with(if last { "m=0" } else { "m=1" }), "{keys}");
        if i == 0 {
            assert!(keys.starts_with("a=t,f=32,s=64,v=64,i=1,q=2"));
        }
        payload.push_str(data);
    }
    assert_eq!(STANDARD.decode(payload).unwrap(), rgba);
}

#[test]
fn rgba_length_mismatch_errors() {
    assert_eq!(
        transmit_rgba(ImageId(1), 2, 2, &[0; 3]),
        Err(GfxError::RgbaLengthMismatch { expected: 16, actual: 3 })
    );
}

#[test]
fn place_has_no_cursor_movement_flag() {
    let out = place(ImageId(7), 2, 10, 1);
    assert_eq!(out, b"\x1b_Ga=p,i=7,p=2,c=10,r=1,q=2,C=1\x1b\\");
}

#[test]
fn delete_all_and_delete_image_sequences() {
    assert_eq!(delete_all_placements(), b"\x1b_Ga=d,d=a,q=2\x1b\\");
    assert_eq!(delete_image(ImageId(9)), b"\x1b_Ga=d,d=I,i=9,q=2\x1b\\");
}

#[test]
fn move_to_is_one_based() {
    assert_eq!(move_to(0, 0), b"\x1b[1;1H");
    assert_eq!(move_to(4, 2), b"\x1b[3;5H");
}
