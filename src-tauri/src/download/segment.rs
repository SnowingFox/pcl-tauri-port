//! Segment planning for multi-connection downloads.

/// Split `total` bytes into at most `max_segments` inclusive `(start, end)` ranges,
/// none smaller than `min_segment` (except when the whole file is smaller).
pub fn plan_segments(total: u64, max_segments: usize, min_segment: u64) -> Vec<(u64, u64)> {
    if total == 0 {
        return vec![];
    }
    let min_segment = min_segment.max(1);
    let by_size = (total / min_segment).max(1) as usize;
    let n = max_segments.max(1).min(by_size) as u64;
    let base = total / n;
    let rem = total % n;
    let mut out = Vec::with_capacity(n as usize);
    let mut start = 0u64;
    for i in 0..n {
        let len = base + if i < rem { 1 } else { 0 };
        out.push((start, start + len - 1));
        start += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(total: u64, ranges: &[(u64, u64)]) {
        assert_eq!(ranges.first().unwrap().0, 0);
        assert_eq!(ranges.last().unwrap().1, total - 1);
        for w in ranges.windows(2) {
            assert_eq!(w[0].1 + 1, w[1].0);
        }
    }

    #[test]
    fn covers_exactly() {
        for (t, m, s) in [(10_000_001u64, 8usize, 1_000_000u64), (100, 4, 10), (1, 8, 1), (7, 3, 1)] {
            let r = plan_segments(t, m, s);
            check(t, &r);
            assert!(r.len() <= m);
        }
    }

    #[test]
    fn respects_minimum_size() {
        let r = plan_segments(3_000_000, 8, 1_000_000);
        assert_eq!(r.len(), 3);
        assert_eq!(plan_segments(500, 8, 1_000_000).len(), 1);
        assert!(plan_segments(0, 8, 10).is_empty());
    }
}
