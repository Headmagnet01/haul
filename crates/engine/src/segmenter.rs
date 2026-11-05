//! Pure split-and-steal logic: how to cut a file into ranges and which range to split.

pub const MIB: u64 = 1024 * 1024;
pub const MIN_STEAL: u64 = MIB;
/// One connection per this many bytes, at first.
pub(crate) const BYTES_PER_INITIAL_CONN: u64 = 4 * MIB;
/// Range boundaries are rounded down to this, so writes stay aligned.
const ALIGN: u64 = 64 * 1024;

/// A byte range `[start, end)`. Bytes before `pos` are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub start: u64,
    pub pos: u64,
    pub end: u64,
}

impl Range {
    pub fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }
    pub fn is_done(&self) -> bool {
        self.pos >= self.end
    }
}

/// Splits `size` bytes into `min(max_conns, size / 4 MiB)` (at least 1) equal ranges.
/// Boundaries are 64 KiB-aligned; the remainder goes to the last range.
pub fn initial_plan(size: u64, max_conns: u32) -> Vec<Range> {
    let n = (size / BYTES_PER_INITIAL_CONN).clamp(1, max_conns.max(1) as u64);
    let mut chunk = size / n;
    if chunk >= ALIGN {
        chunk -= chunk % ALIGN;
    }
    (0..n)
        .map(|i| {
            let start = i * chunk;
            let end = if i == n - 1 { size } else { start + chunk };
            Range { start, pos: start, end }
        })
        .collect()
}

/// Chooses a range to split for an idle connection: the one with the most bytes left
/// (lowest index on ties), if half of it is at least `min_steal`.
/// Returns (victim index, split point).
pub fn pick_steal(ranges: &[Range], min_steal: u64) -> Option<(usize, u64)> {
    let mut best: Option<(usize, u64)> = None;
    for (i, r) in ranges.iter().enumerate() {
        let rem = r.remaining();
        if rem > 0 && best.is_none_or(|(_, b)| rem > b) {
            best = Some((i, rem));
        }
    }
    let (i, rem) = best?;
    let half = rem / 2;
    (half >= min_steal).then(|| (i, ranges[i].pos + half))
}

/// A connection is slow when it runs below `ratio` × the median speed.
pub fn is_slow(speed_bps: f64, median_bps: f64, ratio: f64) -> bool {
    speed_bps < median_bps * ratio
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(start: u64, pos: u64, end: u64) -> Range {
        Range { start, pos, end }
    }

    #[test]
    fn plan_100mib_8() {
        let p = initial_plan(100 * MIB, 8);
        assert_eq!(p.len(), 8);
        assert_eq!(p[0], r(0, 0, 13_107_200));
        assert_eq!(p[7].end, 104_857_600);
        for w in p.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
    }

    #[test]
    fn plan_small_file() {
        assert_eq!(initial_plan(10 * MIB, 8).len(), 2);
    }

    #[test]
    fn plan_tiny_file() {
        assert_eq!(initial_plan(1000, 8), vec![r(0, 0, 1000)]);
    }

    #[test]
    fn plan_remainder_to_last() {
        assert_eq!(initial_plan(8 * MIB + 3, 2), vec![r(0, 0, 4 * MIB), r(4 * MIB, 4 * MIB, 8 * MIB + 3)]);
    }

    #[test]
    fn steal_picks_largest() {
        let ranges = [r(0, 0, 3 * MIB), r(10 * MIB, 10 * MIB, 19 * MIB), r(30 * MIB, 30 * MIB, 35 * MIB)];
        assert_eq!(pick_steal(&ranges, MIN_STEAL), Some((1, 10 * MIB + 9 * MIB / 2)));
    }

    #[test]
    fn steal_tie_picks_lowest_index() {
        let ranges = [r(0, 0, 4 * MIB), r(4 * MIB, 4 * MIB, 8 * MIB)];
        assert_eq!(pick_steal(&ranges, MIN_STEAL).map(|x| x.0), Some(0));
    }

    #[test]
    fn steal_min_size() {
        assert_eq!(pick_steal(&[r(0, 0, 2 * MIB - 1)], MIN_STEAL), None);
        assert_eq!(pick_steal(&[r(0, 0, 2 * MIB)], MIN_STEAL), Some((0, MIB)));
    }

    #[test]
    fn steal_ignores_done() {
        assert_eq!(pick_steal(&[r(0, 5, 5), r(5, 9, 9)], MIN_STEAL), None);
    }

    #[test]
    fn slow_detection() {
        assert!(is_slow(0.24e6, 1e6, 0.25));
        assert!(!is_slow(0.25e6, 1e6, 0.25));
    }
}
