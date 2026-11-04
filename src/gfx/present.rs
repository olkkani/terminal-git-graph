use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Write};

use super::kitty::{self, ImageId};
use super::CellPx;

/// One row image to show; positions are 0-based screen cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RowPlacement {
    pub col: u16,
    pub row: u16,
    pub cols: u16,
    pub key: u64,
    pub rgba_w: u32,
    pub rgba_h: u32,
    /// Kitty z-index: 0 over the text, negative under it.
    pub z: i32,
}

/// Caches transmitted row images (LRU) and shows them with minimal terminal output.
pub struct Presenter {
    max_cached: usize,
    cache: HashMap<u64, ImageId>,
    /// Least recently used first.
    order: VecDeque<u64>,
    last_plan: Option<Vec<RowPlacement>>,
    next_id: u32,
    cell: Option<CellPx>,
}

impl Presenter {
    pub fn new(max_cached: usize) -> Presenter {
        Presenter {
            max_cached,
            cache: HashMap::new(),
            order: VecDeque::new(),
            last_plan: None,
            next_id: 1,
            cell: None,
        }
    }

    pub fn present(
        &mut self,
        out: &mut impl Write,
        cell: CellPx,
        plan: &[RowPlacement],
        render: &mut dyn FnMut(u64) -> Vec<u8>,
    ) -> io::Result<()> {
        if self.cell.is_some_and(|c| c != cell) {
            self.reset(out)?;
        }
        self.cell = Some(cell);
        if self.last_plan.as_deref() == Some(plan) {
            return Ok(());
        }
        let result = self.write_plan(out, plan, render);
        if result.is_err() {
            // The terminal's state is unknown; forget everything we think it holds.
            self.cache.clear();
            self.order.clear();
            self.last_plan = None;
        } else {
            self.last_plan = Some(plan.to_vec());
        }
        result
    }

    fn write_plan(
        &mut self,
        out: &mut impl Write,
        plan: &[RowPlacement],
        render: &mut dyn FnMut(u64) -> Vec<u8>,
    ) -> io::Result<()> {
        let mut buf = b"\x1b7".to_vec();
        buf.extend(kitty::delete_all_placements());
        for (i, p) in plan.iter().enumerate() {
            let id = match self.cache.get(&p.key) {
                Some(&id) => {
                    self.touch(p.key);
                    id
                }
                None => {
                    let id = ImageId(self.next_id);
                    self.next_id += 1;
                    let rgba = render(p.key);
                    let tx = kitty::transmit_rgba(id, p.rgba_w, p.rgba_h, &rgba)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    buf.extend(tx);
                    self.cache.insert(p.key, id);
                    self.order.push_back(p.key);
                    id
                }
            };
            buf.extend(kitty::move_to(p.col, p.row));
            buf.extend(kitty::place(id, i as u32 + 1, p.cols, 1, p.z));
        }
        let in_plan: HashSet<u64> = plan.iter().map(|p| p.key).collect();
        while self.cache.len() > self.max_cached {
            let Some(pos) = self.order.iter().position(|k| !in_plan.contains(k)) else {
                break;
            };
            let key = self.order.remove(pos).expect("position is in range");
            if let Some(id) = self.cache.remove(&key) {
                buf.extend(kitty::delete_image(id));
            }
        }
        buf.extend(b"\x1b8");
        out.write_all(&buf)?;
        out.flush()
    }

    fn touch(&mut self, key: u64) {
        if let Some(pos) = self.order.iter().position(|&k| k == key) {
            self.order.remove(pos);
        }
        self.order.push_back(key);
    }

    /// Assume the terminal dropped all placements (e.g. the screen was cleared), so
    /// the next `present` places every row again; image data stays cached.
    pub fn forget_placements(&mut self) {
        self.last_plan = None;
    }

    /// Remove all placements; image data stays cached.
    pub fn clear(&mut self, out: &mut impl Write) -> io::Result<()> {
        self.last_plan = None;
        out.write_all(&kitty::delete_all_placements())?;
        out.flush()
    }

    /// Remove all placements and images, and empty the cache.
    pub fn reset(&mut self, out: &mut impl Write) -> io::Result<()> {
        let mut buf = kitty::delete_all_placements();
        for id in self.cache.values() {
            buf.extend(kitty::delete_image(*id));
        }
        self.cache.clear();
        self.order.clear();
        self.last_plan = None;
        out.write_all(&buf)?;
        out.flush()
    }
}
