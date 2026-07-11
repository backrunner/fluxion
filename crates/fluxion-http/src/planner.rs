#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentPlan {
    pub index: usize,
    pub start: u64,
    pub end: u64,
}

pub fn plan_segments(total: u64, max_connections: u16, min_split_size: u64) -> Vec<SegmentPlan> {
    // Guard a zero split size (division below) — treat it as 1 byte.
    let min_split_size = min_split_size.max(1);
    if total == 0 || max_connections <= 1 || total <= min_split_size {
        return vec![SegmentPlan {
            index: 0,
            start: 0,
            end: total.saturating_sub(1),
        }];
    }

    // All math in u64: casting `total / min_split_size` to u16 truncated for
    // very large files (>= 65536 splits wrapped around to tiny counts).
    let max_by_size = (total / min_split_size).max(1);
    let count = u64::from(max_connections).min(max_by_size).max(1);
    let base = total / count;
    let mut remainder = total % count;
    let mut start = 0;
    let mut plans = Vec::with_capacity(count as usize);
    for index in 0..count {
        let extra = u64::from(remainder > 0);
        remainder = remainder.saturating_sub(1);
        let len = base + extra;
        let end = start + len - 1;
        plans.push(SegmentPlan {
            index: index as usize,
            start,
            end,
        });
        start = end + 1;
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_cover_file_without_gaps() {
        let plans = plan_segments(100, 4, 1);
        assert_eq!(plans.first().unwrap().start, 0);
        assert_eq!(plans.last().unwrap().end, 99);
        for pair in plans.windows(2) {
            assert_eq!(pair[0].end + 1, pair[1].start);
        }
    }

    #[test]
    fn small_file_single_segment() {
        assert_eq!(plan_segments(100, 16, 1024).len(), 1);
    }
}
