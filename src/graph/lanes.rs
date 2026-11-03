//! Assigns commits to vertical lanes for drawing the graph.

use crate::git::log::Commit;

pub const PALETTE_LEN: usize = 8;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LaneSeg {
    pub lane: usize,
    pub color: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub struct GraphRow {
    pub hash: String,
    pub lane: usize,
    pub color: usize,
    /// Lanes passing vertically through this row, unrelated to the node.
    pub pass: Vec<LaneSeg>,
    /// Lanes coming down from above that end in this node (own lane excluded).
    pub joins: Vec<LaneSeg>,
    /// Lanes leaving this node downward toward the 2nd and later parents.
    pub forks: Vec<LaneSeg>,
    /// Whether the node's own lane continues down to its first parent.
    pub continues: bool,
    pub is_merge: bool,
    /// Number of lanes this row occupies.
    pub width: usize,
}

/// Builds graph rows one commit at a time; commits must arrive children first.
#[derive(Default)]
pub struct LaneBuilder {
    /// Per lane: the hash it waits for and its color. `None` is a free slot.
    lanes: Vec<Option<(String, usize)>>,
    next_color: usize,
}

impl LaneBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, c: &Commit) -> GraphRow {
        let waiting: Vec<usize> = (0..self.lanes.len())
            .filter(|&i| matches!(&self.lanes[i], Some((h, _)) if *h == c.hash))
            .collect();
        let pass: Vec<LaneSeg> = self
            .lanes
            .iter()
            .enumerate()
            .filter(|(i, _)| !waiting.contains(i))
            .filter_map(|(i, l)| {
                l.as_ref().map(|(_, color)| LaneSeg {
                    lane: i,
                    color: *color,
                })
            })
            .collect();

        let (lane, color) = match waiting.first() {
            Some(&i) => (i, self.lanes[i].as_ref().unwrap().1),
            None => {
                let i = self.free_slot();
                let color = self.new_color();
                self.lanes[i] = Some((String::new(), color));
                (i, color)
            }
        };
        let joins: Vec<LaneSeg> = waiting
            .iter()
            .skip(1)
            .map(|&i| LaneSeg {
                lane: i,
                color: self.lanes[i].as_ref().unwrap().1,
            })
            .collect();

        // Reserve lanes closed by this row's joins so forks below don't reuse them
        // in the same row; they are freed after the forks are placed.
        for j in &joins {
            self.lanes[j.lane] = Some((String::new(), j.color));
        }

        let continues = match c.parents.first() {
            Some(p) => {
                self.lanes[lane] = Some((p.clone(), color));
                true
            }
            None => {
                self.lanes[lane] = None;
                false
            }
        };

        let mut forks = Vec::new();
        for p in c.parents.iter().skip(1) {
            let existing = (0..self.lanes.len())
                .find(|&i| i != lane && matches!(&self.lanes[i], Some((h, _)) if h == p));
            let seg = match existing {
                Some(i) => LaneSeg {
                    lane: i,
                    color: self.lanes[i].as_ref().unwrap().1,
                },
                None => {
                    let i = self.free_slot();
                    let color = self.new_color();
                    self.lanes[i] = Some((p.clone(), color));
                    LaneSeg { lane: i, color }
                }
            };
            forks.push(seg);
        }

        for j in &joins {
            self.lanes[j.lane] = None;
        }

        let width = std::iter::once(lane)
            .chain(pass.iter().chain(&joins).chain(&forks).map(|s| s.lane))
            .max()
            .map_or(0, |m| m + 1);
        while matches!(self.lanes.last(), Some(None)) {
            self.lanes.pop();
        }

        GraphRow {
            hash: c.hash.clone(),
            lane,
            color,
            pass,
            joins,
            forks,
            continues,
            is_merge: c.parents.len() > 1,
            width,
        }
    }

    /// Leftmost free lane, appending one if all are taken.
    fn free_slot(&mut self) -> usize {
        match self.lanes.iter().position(Option::is_none) {
            Some(i) => i,
            None => {
                self.lanes.push(None);
                self.lanes.len() - 1
            }
        }
    }

    fn new_color(&mut self) -> usize {
        let color = self.next_color % PALETTE_LEN;
        self.next_color += 1;
        color
    }
}
