//! Property-based tests for the segment table (Q34, Phase 9).
//!
//! The segment table is the heart of the acceleration: it decides
//! which bytes each connection downloads, and the work-stealing
//! splits (Q32) rewrite it while downloads run. A bug here loses
//! bytes, downloads a byte twice, or overlaps two connections on
//! the same range — and a fixed example rarely hits the edge.
//! These tests throw randomized plans, progress, and split
//! sequences at the pure `segments` API and assert the invariants
//! that must hold after every operation:
//!
//! * the segments cover `[0, total)` exactly once (no gap, no
//!   overlap) — `validate_segment_map`;
//! * `downloaded()` never exceeds `total`, and equals `total`
//!   exactly when every segment is finished;
//! * splitting never changes how many bytes are downloaded or
//!   how many remain.
//!
//! The integration tests in `tests/download.rs` cover the same
//! invariants end-to-end against a local HTTP server; these cover
//! the table itself, thousands of shapes a server test never
//! reaches. (The shared "testserver" module — one reusable fixture
//! server for the hard cases instead of the per-file inline
//! servers — is still open for Phase 10; see Q34.)

use proptest::prelude::*;
use swiftget_engine::segments::{
    all_finished, claim_work_eta, claim_work_split, downloaded, plan, split_largest_remaining,
    split_slowest_remaining, validate_segment_map, Segment,
};

// Any plan partitions the file: exact coverage, nothing downloaded yet.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]
    #[test]
    fn plan_partitions_file(
        total in 0u64..=10_000_000u64,
        connections in 1u32..=32u32,
        min_size in 1u64..=1_000_000u64,
    ) {
        let segs = plan(total, connections, min_size);
        prop_assert!(validate_segment_map(&segs, total));
        prop_assert_eq!(downloaded(&segs), 0);
        let covered: u64 = segs.iter().map(|s| s.end - s.start).sum();
        prop_assert_eq!(covered, total);
        if total > 0 {
            prop_assert!(!segs.is_empty());
            prop_assert!(segs.len() as u64 <= connections.max(1) as u64);
        }
    }
}

// Random progress + random work-stealing splits keep the map valid
// and never change the byte accounting.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]
    #[test]
    fn random_progress_and_splits_keep_map_valid(
        total in 1u64..=1_000_000u64,
        connections in 1u32..=16u32,
        min_size in 1u64..=100_000u64,
        steps in 0usize..=40usize,
        // per-step: which segment advances, by how much, and which
        // claim/split the free connection tries (0 = split, 1 = ETA, 2 = largest)
        ops in prop::collection::vec((0usize..=15usize, 0u64..=50_000u64, 0u8..=2u8), 0..=40),
    ) {
        let mut segs = plan(total, connections, min_size);
        for s in segs.iter_mut() {
            s.active = true;
        }
        prop_assert!(validate_segment_map(&segs, total));
        let mut done_before = 0u64;
        for step in 0..steps {
            let (seg_idx, advance, op) = ops.get(step).copied().unwrap_or((0, 0, 0));
            // advance one active, unfinished segment (clamped to its end)
            if !segs.is_empty() {
                let i = seg_idx % segs.len();
                if segs[i].active && !segs[i].is_finished() {
                    let adv = advance.min(segs[i].remaining());
                    segs[i].pos += adv;
                }
            }
            // a free connection takes work (split, ETA-steal, or largest)
            match op {
                0 => { claim_work_split(&mut segs, min_size.max(1)); }
                1 => {
                    let speeds: Vec<(usize, f64)> = segs
                        .iter()
                        .enumerate()
                        .map(|(i, _)| (i, ((i * 7919) % 997) as f64 + 1.0))
                        .collect();
                    claim_work_eta(&mut segs, min_size.max(1), &speeds);
                }
                _ => { split_largest_remaining(&mut segs, min_size.max(1)); }
            }
            prop_assert!(
                validate_segment_map(&segs, total),
                "map invalid after step {step}: {segs:?}"
            );
            let done = downloaded(&segs);
            prop_assert!(done <= total, "downloaded {done} > total {total}");
            prop_assert!(
                done >= done_before,
                "downloaded went backwards: {done} < {done_before}"
            );
            done_before = done;
            prop_assert_eq!(all_finished(&segs), done == total);
        }
        // splitting alone never creates or destroys bytes: the sum of
        // (downloaded + remaining) is the file size at every step
        let remaining: u64 = segs.iter().map(Segment::remaining).sum();
        prop_assert_eq!(downloaded(&segs) + remaining, total);
    }
}

// Splits without any progress keep the table exact: same coverage,
// still nothing downloaded, still valid.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn splits_alone_never_lose_bytes(
        total in 1u64..=1_000_000u64,
        connections in 1u32..=8u32,
        min_size in 1u64..=10_000u64,
        splits in 0usize..=32usize,
    ) {
        let mut segs = plan(total, connections, min_size);
        for s in segs.iter_mut() {
            s.active = true;
        }
        for _ in 0..splits {
            // ETA-steal with no speeds at all falls back to the
            // largest remainder, so it exercises both paths
            split_slowest_remaining(&mut segs, min_size.max(2), &[]);
            prop_assert!(validate_segment_map(&segs, total));
            prop_assert_eq!(downloaded(&segs), 0);
        }
        let covered: u64 = segs.iter().map(|s| s.end - s.start).sum();
        prop_assert_eq!(covered, total);
    }
}
