//! Segment table: how a file is split between connections.
//!
//! This is the heart of the acceleration:
//! 1. The file is split into N equal parts, one per connection.
//! 2. When a connection finishes its part early, it does not sit idle:
//!    it takes the part with the most bytes still remaining and splits it
//!    in half, then downloads the second half itself (dynamic segmentation).
//!
//! `end` is exclusive. A segment is finished when `pos >= end`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub start: u64,
    pub pos: u64,
    pub end: u64,
    /// A connection is currently working on this segment (runtime only).
    #[serde(skip)]
    pub active: bool,
    /// The stall monitor flagged this segment: its connection must be
    /// closed and the segment re-requested from `pos` (runtime only).
    #[serde(skip)]
    pub takeover: bool,
}

impl Segment {
    pub fn new(start: u64, end: u64) -> Self {
        Segment {
            start,
            pos: start,
            end,
            active: false,
            takeover: false,
        }
    }
    pub fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }
    pub fn done(&self) -> u64 {
        self.pos.saturating_sub(self.start)
    }
    pub fn is_finished(&self) -> bool {
        self.pos >= self.end
    }
}

/// Split `total` bytes into at most `connections` parts, never making a part
/// smaller than `min_size` (except when the whole file is smaller).
pub fn plan(total: u64, connections: u32, min_size: u64) -> Vec<Segment> {
    if total == 0 {
        return vec![Segment::new(0, 0)];
    }
    let min_size = min_size.max(1);
    let by_size = (total / min_size).max(1);
    let n = (connections.max(1) as u64).min(by_size);
    let part = total / n;
    (0..n)
        .map(|i| {
            let start = i * part;
            let end = if i == n - 1 { total } else { (i + 1) * part };
            Segment::new(start, end)
        })
        .collect()
}

/// Total bytes already downloaded across all segments.
pub fn downloaded(segs: &[Segment]) -> u64 {
    segs.iter().map(|s| s.done()).sum()
}

pub fn all_finished(segs: &[Segment]) -> bool {
    segs.iter().all(|s| s.is_finished())
}

/// Give a free connection something to do. First it takes any unfinished
/// segment nobody is working on (e.g. after resume); otherwise it splits the
/// active segment with the most remaining bytes. Returns the segment index
/// the caller now owns (already marked active), or `None` when there is
/// nothing worth taking.
pub fn claim_work(segs: &mut Vec<Segment>, min_split: u64) -> Option<usize> {
    if let Some(i) = segs.iter().position(|s| !s.active && !s.is_finished()) {
        segs[i].active = true;
        return Some(i);
    }
    split_largest(segs, min_split)
}

/// Split the active segment with the largest remaining range in half.
/// Only splits when both halves would be at least `min_split` bytes.
pub fn split_largest(segs: &mut Vec<Segment>, min_split: u64) -> Option<usize> {
    let min_split = min_split.max(1);
    let (idx, rem) = segs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.active && !s.is_finished())
        .map(|(i, s)| (i, s.remaining()))
        .max_by_key(|(_, r)| *r)?;
    if rem < min_split * 2 {
        return None;
    }
    let mid = segs[idx].pos + rem / 2;
    let old_end = segs[idx].end;
    segs[idx].end = mid;
    let mut new_seg = Segment::new(mid, old_end);
    new_seg.active = true;
    segs.push(new_seg);
    Some(segs.len() - 1)
}

/// Split the active segment with the largest remaining range at the
/// midpoint of what is left, provided at least `min_remaining` bytes
/// remain. The boundary move and the creation of the new segment happen
/// in one call, while the caller holds the table lock, so no byte can
/// be written twice or skipped: the old segment stops at the midpoint
/// and the new one starts exactly there. The new segment is already in
/// the table before any worker writes to its range.
pub fn split_largest_remaining(segs: &mut Vec<Segment>, min_remaining: u64) -> Option<usize> {
    let (idx, rem) = segs
        .iter()
        .enumerate()
        .filter(|(_, s)| s.active && !s.is_finished())
        .map(|(i, s)| (i, s.remaining()))
        .max_by_key(|(_, r)| *r)?;
    if rem < min_remaining.max(2) {
        return None;
    }
    let mid = segs[idx].pos + rem / 2;
    if mid <= segs[idx].pos || mid >= segs[idx].end {
        return None;
    }
    let old_end = segs[idx].end;
    segs[idx].end = mid;
    let mut new_seg = Segment::new(mid, old_end);
    new_seg.active = true;
    segs.push(new_seg);
    Some(segs.len() - 1)
}

/// Give a free connection something to do, using the adaptive split
/// threshold: idle segments first, otherwise the largest active
/// segment, cut at the midpoint when enough remains.
pub fn claim_work_split(segs: &mut Vec<Segment>, min_remaining: u64) -> Option<usize> {
    if let Some(i) = segs.iter().position(|s| !s.active && !s.is_finished()) {
        segs[i].active = true;
        return Some(i);
    }
    split_largest_remaining(segs, min_remaining)
}

/// Is this segment table a faithful record of a `total`-byte file?
/// Every segment must lie inside the file, its progress must lie
/// inside the segment, and together the segments must cover the whole
/// file exactly once: no gap, no overlap. A table that fails any of
/// these checks must not be resumed from — the caller re-plans from
/// scratch instead of trusting it, so corrupt data is never kept.
pub fn validate_segment_map(segs: &[Segment], total: u64) -> bool {
    if total == 0 {
        return segs.len() == 1 && segs[0].start == 0 && segs[0].pos == 0 && segs[0].end == 0;
    }
    if segs.is_empty() {
        return false;
    }
    for s in segs {
        if s.start >= s.end || s.end > total {
            return false;
        }
        if s.pos < s.start || s.pos > s.end {
            return false;
        }
    }
    let mut spans: Vec<(u64, u64)> = segs.iter().map(|s| (s.start, s.end)).collect();
    spans.sort_unstable();
    if spans[0].0 != 0 || spans.last().unwrap().1 != total {
        return false;
    }
    for w in spans.windows(2) {
        if w[0].1 != w[1].0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_covers_whole_file() {
        let segs = plan(1000, 8, 10);
        assert_eq!(segs.len(), 8);
        assert_eq!(segs[0].start, 0);
        assert_eq!(segs.last().unwrap().end, 1000);
        for w in segs.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
    }

    #[test]
    fn plan_respects_min_size() {
        let segs = plan(1000, 8, 400);
        assert_eq!(segs.len(), 2);
        let segs = plan(100, 8, 400);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].end, 100);
    }

    #[test]
    fn claim_prefers_idle_segments() {
        let mut segs = plan(1000, 4, 10);
        let a = claim_work(&mut segs, 10).unwrap();
        let b = claim_work(&mut segs, 10).unwrap();
        assert_ne!(a, b);
        assert!(segs[a].active && segs[b].active);
    }

    #[test]
    fn dynamic_split_takes_largest_remaining() {
        let mut segs = plan(1000, 2, 10);
        for s in segs.iter_mut() {
            s.active = true;
        }
        // first segment finished, second only 100 bytes in
        segs[0].pos = segs[0].end;
        segs[0].active = false;
        segs[1].pos = 600;
        let i = claim_work(&mut segs, 10).unwrap();
        assert_eq!(i, 2);
        assert_eq!(segs[1].end, 800);
        assert_eq!(segs[2].start, 800);
        assert_eq!(segs[2].end, 1000);
        // ranges still cover everything without overlap
        assert_eq!(segs[1].remaining() + segs[2].remaining(), 400);
    }

    #[test]
    fn no_split_when_too_small() {
        let mut segs = vec![Segment::new(0, 100)];
        segs[0].active = true;
        segs[0].pos = 90;
        assert!(claim_work(&mut segs, 10).is_none());
    }

    #[test]
    fn downloaded_sum() {
        let mut segs = plan(1000, 4, 10);
        segs[0].pos += 50;
        segs[3].pos += 25;
        assert_eq!(downloaded(&segs), 75);
        assert!(!all_finished(&segs));
    }

    #[test]
    fn validate_accepts_a_sound_map_in_any_order() {
        // splits append, so a real table is rarely sorted
        let mut segs = plan(1000, 4, 10);
        segs[0].pos = 100;
        segs.swap(0, 3);
        assert!(validate_segment_map(&segs, 1000));
    }

    #[test]
    fn validate_rejects_gap_overlap_and_bad_positions() {
        let good = plan(1000, 4, 10);
        assert!(validate_segment_map(&good, 1000));
        let mut gap = good.clone();
        gap[1].start = 251;
        assert!(!validate_segment_map(&gap, 1000));
        let mut overlap = good.clone();
        overlap[1].start = 249;
        assert!(!validate_segment_map(&overlap, 1000));
        let mut beyond = good.clone();
        beyond[3].end = 1001;
        assert!(!validate_segment_map(&beyond, 1000));
        let mut pos_out = good.clone();
        pos_out[0].pos = 251;
        assert!(!validate_segment_map(&pos_out, 1000));
        let mut pos_back = good.clone();
        pos_back[1].pos = 100;
        assert!(!validate_segment_map(&pos_back, 1000));
        assert!(!validate_segment_map(&[], 1000));
        assert!(!validate_segment_map(&good, 999));
        let mut empty_seg = good.clone();
        empty_seg[1].end = empty_seg[1].start;
        assert!(!validate_segment_map(&empty_seg, 1000));
    }

    #[test]
    fn validate_empty_file() {
        assert!(validate_segment_map(&plan(0, 4, 10), 0));
        assert!(!validate_segment_map(&plan(1000, 4, 10), 0));
    }

    #[test]
    fn adaptive_split_needs_one_megabyte_remaining() {
        let mut segs = vec![Segment::new(0, 3 * 1024 * 1024)];
        segs[0].active = true;
        segs[0].pos = 512 * 1024;
        let i = split_largest_remaining(&mut segs, 1024 * 1024).unwrap();
        // remaining was 2.5 MB; the cut is at the midpoint of what is left
        assert_eq!(segs[0].end, 512 * 1024 + (3 * 1024 * 1024 - 512 * 1024) / 2);
        assert_eq!(segs[i].start, segs[0].end);
        assert_eq!(segs[i].end, 3 * 1024 * 1024);
        assert_eq!(
            segs[0].remaining() + segs[i].remaining(),
            3 * 1024 * 1024 - 512 * 1024
        );
        // now only 1 MB - 1 byte remains in the largest segment: no split
        segs[0].pos = segs[0].end - 1;
        segs[i].pos = segs[i].end - 1;
        assert!(split_largest_remaining(&mut segs, 1024 * 1024).is_none());
    }

    #[test]
    fn claim_work_split_prefers_idle_then_splits() {
        let mut segs = plan(4 * 1024 * 1024, 2, 10);
        let a = claim_work_split(&mut segs, 1024 * 1024).unwrap();
        let b = claim_work_split(&mut segs, 1024 * 1024).unwrap();
        assert_ne!(a, b);
        segs[a].pos = segs[a].end; // a finished
        segs[a].active = false;
        let c = claim_work_split(&mut segs, 1024 * 1024).unwrap();
        assert_eq!(c, 2, "the remaining segment is cut in half");
        assert_eq!(segs[b].end, segs[c].start);
    }

    #[test]
    fn split_while_writing_keeps_coverage_and_no_overlap() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        // one writer advancing pos in small steps, one splitter cutting
        // the segment mid-write, exactly as the download loop does
        let table = Arc::new(Mutex::new(vec![Segment::new(0, 8 * 1024 * 1024)]));
        table.lock().unwrap()[0].active = true;
        let writer_table = table.clone();
        let writer = thread::spawn(move || {
            for _ in 0..2000 {
                let mut t = writer_table.lock().unwrap();
                let s = &mut t[0];
                if s.pos < s.end {
                    s.pos = (s.pos + 1024).min(s.end);
                }
            }
        });
        let splitter_table = table.clone();
        let splitter = thread::spawn(move || {
            for _ in 0..200 {
                let mut t = splitter_table.lock().unwrap();
                // only ever cut segment 0 while it is still the writer's
                if t[0].remaining() >= 1024 * 1024 {
                    let mid = t[0].pos + t[0].remaining() / 2;
                    let old_end = t[0].end;
                    t[0].end = mid;
                    t.push(Segment::new(mid, old_end));
                }
                // invariant under the lock: coverage, no overlap, pos in range
                assert!(validate_segment_map(&t, 8 * 1024 * 1024));
            }
        });
        writer.join().unwrap();
        splitter.join().unwrap();
        let t = table.lock().unwrap();
        assert!(validate_segment_map(&t, 8 * 1024 * 1024));
    }
}
