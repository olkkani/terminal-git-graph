use base64::{engine::general_purpose::STANDARD, Engine};

use super::GfxError;

const MAX_CHUNK: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ImageId(pub u32);

fn apc(keys: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(keys.len() + payload.len() + 6);
    out.extend_from_slice(b"\x1b_G");
    out.extend_from_slice(keys.as_bytes());
    if !payload.is_empty() {
        out.push(b';');
        out.extend_from_slice(payload);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

/// Transmit raw RGBA pixels (without displaying them), chunked at 4096 base64 bytes.
pub fn transmit_rgba(
    id: ImageId,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<Vec<u8>, GfxError> {
    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(GfxError::RgbaLengthMismatch {
            expected,
            actual: rgba.len(),
        });
    }
    let encoded = STANDARD.encode(rgba);
    let mut chunks: Vec<&[u8]> = encoded.as_bytes().chunks(MAX_CHUNK).collect();
    if chunks.is_empty() {
        chunks.push(&[]);
    }
    let last = chunks.len() - 1;
    let mut out = Vec::new();
    for (i, chunk) in chunks.into_iter().enumerate() {
        let more = u8::from(i != last);
        let keys = if i == 0 {
            format!("a=t,f=32,s={width},v={height},i={},q=2,m={more}", id.0)
        } else {
            format!("m={more}")
        };
        out.extend(apc(&keys, chunk));
    }
    Ok(out)
}

/// Place an image over `cols` x `rows` cells at the cursor without moving the cursor.
pub fn place(id: ImageId, placement: u32, cols: u16, rows: u16) -> Vec<u8> {
    apc(
        &format!("a=p,i={},p={placement},c={cols},r={rows},q=2,C=1", id.0),
        &[],
    )
}

pub fn delete_all_placements() -> Vec<u8> {
    apc("a=d,d=a,q=2", &[])
}

pub fn delete_image(id: ImageId) -> Vec<u8> {
    apc(&format!("a=d,d=I,i={},q=2", id.0), &[])
}

/// Move the cursor; inputs are 0-based, the CSI sequence is 1-based.
pub fn move_to(col: u16, row: u16) -> Vec<u8> {
    format!("\x1b[{};{}H", u32::from(row) + 1, u32::from(col) + 1).into_bytes()
}
