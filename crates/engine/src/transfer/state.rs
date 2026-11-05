//! The shared segment table: which byte ranges exist, how far each got, and who owns it.

use crate::segmenter::{Range, pick_steal};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Slot {
    pub range: Range,
    pub owner: Option<usize>,
}

pub(crate) struct SegTable {
    pub slots: Vec<Slot>,
}

impl SegTable {
    pub fn new(ranges: Vec<Range>) -> Self {
        SegTable { slots: ranges.into_iter().map(|range| Slot { range, owner: None }).collect() }
    }

    pub fn ranges(&self) -> Vec<Range> {
        let mut v: Vec<Range> = self.slots.iter().map(|s| s.range).collect();
        v.sort_by_key(|r| r.start);
        v
    }

    pub fn written(&self) -> u64 {
        self.slots.iter().map(|s| s.range.pos - s.range.start).sum()
    }

    pub fn all_done(&self) -> bool {
        self.slots.iter().all(|s| s.range.is_done())
    }

    /// Takes the first unfinished range nobody owns.
    pub fn claim(&mut self, conn: usize) -> Option<usize> {
        let i = self.slots.iter().position(|s| s.owner.is_none() && !s.range.is_done())?;
        self.slots[i].owner = Some(conn);
        Some(i)
    }

    /// Splits the range with the most bytes left and gives the second half to `conn`.
    /// Returns (new slot index, stolen bytes).
    pub fn steal(&mut self, conn: usize, min_steal: u64) -> Option<(usize, u64)> {
        let ranges: Vec<Range> = self.slots.iter().map(|s| s.range).collect();
        let (victim, split) = pick_steal(&ranges, min_steal)?;
        let old_end = self.slots[victim].range.end;
        self.slots[victim].range.end = split;
        self.slots.push(Slot { range: Range { start: split, pos: split, end: old_end }, owner: Some(conn) });
        Some((self.slots.len() - 1, old_end - split))
    }

    pub fn release(&mut self, idx: usize) {
        self.slots[idx].owner = None;
    }
}

/// True when saved ranges tile `[0, size)` exactly, so they can be resumed.
pub(crate) fn ranges_valid(ranges: &[Range], size: u64) -> bool {
    if ranges.is_empty() || ranges[0].start != 0 {
        return false;
    }
    let tiles = ranges.windows(2).all(|w| w[0].end == w[1].start);
    let sane = ranges.iter().all(|r| r.start <= r.pos && r.pos <= r.end);
    tiles && sane && ranges.last().map(|r| r.end) == Some(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segmenter::MIB;

    #[test]
    fn steal_splits_and_assigns() {
        let mut t = SegTable::new(vec![Range { start: 0, pos: 0, end: 4 * MIB }]);
        assert_eq!(t.claim(0), Some(0));
        assert_eq!(t.claim(1), None);
        let (i, bytes) = t.steal(1, MIB).unwrap();
        assert_eq!(bytes, 2 * MIB);
        assert_eq!(t.slots[0].range.end, 2 * MIB);
        assert_eq!(t.slots[i].range, Range { start: 2 * MIB, pos: 2 * MIB, end: 4 * MIB });
        assert!(ranges_valid(&t.ranges(), 4 * MIB));
    }

    #[test]
    fn invalid_ranges_rejected() {
        let r = |s, p, e| Range { start: s, pos: p, end: e };
        assert!(!ranges_valid(&[], 10));
        assert!(!ranges_valid(&[r(0, 0, 5), r(6, 6, 10)], 10));
        assert!(!ranges_valid(&[r(0, 0, 5), r(5, 5, 9)], 10));
        assert!(!ranges_valid(&[r(0, 7, 5), r(5, 5, 10)], 10));
        assert!(ranges_valid(&[r(0, 5, 5), r(5, 6, 10)], 10));
    }
}
