//! # ternary-diff
//!
//! Diff, patch, and three-way merge for sequences of ternary values
//! (`{-1, 0, +1}`).
//!
//! ## When to use this
//!
//! Use `ternary-diff` when your data is naturally a sequence of ternary values
//! — sensor streams, voting tallies, cell/automata states, ternary feature
//! flags — and you need to **compare** two versions, **transform** one into the
//! other, or **merge** two divergent edits against a common base. It is, in
//! effect, `git diff` / `git merge` for ternary data rather than text lines.
//!
//! What it provides:
//! - [`Trit`] — the core ternary value (Neg, Zero, Pos).
//! - [`TernarySeq`] — an ordered, mutable sequence of trits.
//! - [`TernaryDiff`] — an LCS-based diff between two sequences.
//! - [`TernaryPatch`] — apply a diff to a sequence (reconstructing the target)
//!   and reverse diffs.
//! - [`ThreeWayMerge`] — position-wise three-way merge with conflict detection.
//! - [`ConflictResolver`] — strategies for resolving merge conflicts.
//!
//! ## Round-trip guarantee
//!
//! For any two sequences `a` and `b`, applying the diff to `a` reproduces `b`
//! exactly, and applying the reversed diff to `b` reproduces `a` exactly:
//!
//! ```ignore
//! let d = TernaryDiff::diff(&a, &b);
//! assert_eq!(TernaryPatch::apply(&a, &d).trits(), b.trits());
//! assert_eq!(TernaryPatch::apply(&b, &TernaryPatch::reverse(&d)).trits(), a.trits());
//! ```

// Encourage (but do not hard-deny) documentation of the entire public API.
#![warn(missing_docs)]

/// A single ternary value: one of negative, zero, or positive.
///
/// Maps to the integers `-1`, `0`, and `+1` via [`Trit::to_i8`] /
/// [`Trit::from_i8`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trit {
    /// The negative value (`-1`).
    Neg,
    /// The zero / neutral value (`0`).
    Zero,
    /// The positive value (`+1`).
    Pos,
}

impl Trit {
    /// Convert this trit to its `i8` representation (`-1`, `0`, or `1`).
    pub fn to_i8(self) -> i8 {
        match self {
            Trit::Neg => -1,
            Trit::Zero => 0,
            Trit::Pos => 1,
        }
    }

    /// Parse an `i8` into a trit. Returns `None` for any value other than
    /// `-1`, `0`, or `1`.
    pub fn from_i8(v: i8) -> Option<Self> {
        match v {
            -1 => Some(Trit::Neg),
            0 => Some(Trit::Zero),
            1 => Some(Trit::Pos),
            _ => None,
        }
    }
}

/// An ordered, mutable sequence of [`Trit`] values.
#[derive(Debug, Clone, PartialEq)]
pub struct TernarySeq {
    trits: Vec<Trit>,
}

impl TernarySeq {
    /// Create a sequence that owns the given trits.
    pub fn new(trits: Vec<Trit>) -> Self {
        TernarySeq { trits }
    }

    /// Build a sequence from a slice of `i8`, **silently dropping** any value
    /// that is not `-1`, `0`, or `1`.
    ///
    /// This is forgiving but can hide data-entry mistakes. If you need to
    /// detect rejected values, use [`TernarySeq::from_i8_strict`].
    pub fn from_i8(values: &[i8]) -> Self {
        TernarySeq {
            trits: values.iter().filter_map(|&v| Trit::from_i8(v)).collect(),
        }
    }

    /// Build a sequence from a slice of `i8`, returning an error holding the
    /// first value that is not `-1`, `0`, or `1` instead of silently dropping
    /// it.
    ///
    /// ```
    /// # use ternary_diff::{TernarySeq, Trit};
    /// assert!(TernarySeq::from_i8_strict(&[-1, 0, 1]).is_ok());
    /// assert_eq!(TernarySeq::from_i8_strict(&[1, 2, 3]), Err(2));
    /// ```
    pub fn from_i8_strict(values: &[i8]) -> Result<Self, i8> {
        let mut trits = Vec::with_capacity(values.len());
        for &v in values {
            match Trit::from_i8(v) {
                Some(t) => trits.push(t),
                // Surface the offending value instead of silently dropping it.
                None => return Err(v),
            }
        }
        Ok(TernarySeq::new(trits))
    }

    /// Number of trits in the sequence.
    pub fn len(&self) -> usize {
        self.trits.len()
    }

    /// Whether the sequence contains no trits.
    pub fn is_empty(&self) -> bool {
        self.trits.is_empty()
    }

    /// Borrow the underlying trit slice.
    pub fn trits(&self) -> &[Trit] {
        &self.trits
    }

    /// Get the trit at `idx`, or `None` if out of bounds.
    pub fn get(&self, idx: usize) -> Option<Trit> {
        self.trits.get(idx).copied()
    }

    /// Set the trit at `idx`.
    ///
    /// If `idx` is out of bounds this is a silent no-op; check bounds with
    /// [`TernarySeq::get`] beforehand if that matters to you.
    pub fn set(&mut self, idx: usize, trit: Trit) {
        if idx < self.trits.len() {
            self.trits[idx] = trit;
        }
    }

    /// Append a trit to the end of the sequence.
    pub fn push(&mut self, trit: Trit) {
        self.trits.push(trit);
    }

    /// Insert a trit at `idx`, shifting later elements right.
    pub fn insert(&mut self, idx: usize, trit: Trit) {
        self.trits.insert(idx, trit);
    }

    /// Remove and return the trit at `idx`, or `None` if out of bounds.
    pub fn remove(&mut self, idx: usize) -> Option<Trit> {
        if idx < self.trits.len() {
            Some(self.trits.remove(idx))
        } else {
            None
        }
    }

    /// Return a copy of the sub-sequence `trits[start..end]`.
    ///
    /// Both bounds are clamped to the sequence length and `start` is clamped to
    /// `end`, so this never panics: an empty or inverted range yields an empty
    /// sequence.
    pub fn slice(&self, start: usize, end: usize) -> TernarySeq {
        let len = self.trits.len();
        let end = end.min(len);
        let start = start.min(end);
        TernarySeq::new(self.trits[start..end].to_vec())
    }
}

/// A single operation within a diff.
///
/// Positions are reported relative to the relevant sequence: for [`DiffOp::Equal`],
/// [`DiffOp::Change`], and [`DiffOp::Delete`] the `pos` is an index into the
/// *original* (old) sequence; for [`DiffOp::Insert`] it is an index into the
/// *new* (target) sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffOp {
    /// An element present (and unchanged) in both sequences.
    Equal {
        /// Index of the element in the original sequence.
        pos: usize,
        /// The unchanged value.
        trit: Trit,
    },
    /// An element whose value was replaced.
    Change {
        /// Index of the replaced element in the original sequence.
        pos: usize,
        /// The original value.
        old: Trit,
        /// The replacement value.
        new: Trit,
    },
    /// A new element added to the sequence.
    Insert {
        /// Index of the inserted element in the new sequence.
        pos: usize,
        /// The inserted value.
        trit: Trit,
    },
    /// An element removed from the sequence.
    Delete {
        /// Index of the removed element in the original sequence.
        pos: usize,
        /// The removed value.
        trit: Trit,
    },
}

/// The result of diffing two sequences: an ordered list of [`DiffOp`]s.
#[derive(Debug, Clone)]
pub struct TernaryDiff {
    ops: Vec<DiffOp>,
}

impl TernaryDiff {
    /// Compute the diff between `old` and `new` using the classic LCS
    /// (Longest Common Subsequence) dynamic-programming algorithm.
    ///
    /// The LCS table is backtracked to produce `Equal` / `Insert` / `Delete`
    /// operations, then a normalization pass pairs adjacent `Delete` + `Insert`
    /// runs into `Change` operations when they represent value replacements at
    /// aligned positions. Any leftover inserts/deletes are kept as structural
    /// additions/removals.
    pub fn diff(old: &TernarySeq, new: &TernarySeq) -> Self {
        let m = old.len();
        let n = new.len();

        // Build the LCS length table. dp[i][j] = LCS length of old[..i] and
        // new[..j].
        let mut dp = vec![vec![0usize; n + 1]; m + 1];
        for i in 1..=m {
            for j in 1..=n {
                if old.get(i - 1) == new.get(j - 1) {
                    dp[i][j] = dp[i - 1][j - 1] + 1;
                } else {
                    dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
                }
            }
        }

        // Backtrack from (m, n) to (0, 0) recovering the edit script.
        let mut ops = Vec::new();
        let mut i = m;
        let mut j = n;

        while i > 0 || j > 0 {
            if i > 0 && j > 0 && old.get(i - 1) == new.get(j - 1) {
                ops.push(DiffOp::Equal {
                    pos: i - 1,
                    trit: old.get(i - 1).unwrap(),
                });
                i -= 1;
                j -= 1;
            } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
                ops.push(DiffOp::Insert {
                    pos: j - 1,
                    trit: new.get(j - 1).unwrap(),
                });
                j -= 1;
            } else if i > 0 {
                ops.push(DiffOp::Delete {
                    pos: i - 1,
                    trit: old.get(i - 1).unwrap(),
                });
                i -= 1;
            }
        }

        ops.reverse();

        // Pair adjacent delete/insert runs into changes where appropriate.
        let normalized = Self::normalize_ops(&ops);
        TernaryDiff { ops: normalized }
    }

    fn normalize_ops(ops: &[DiffOp]) -> Vec<DiffOp> {
        let mut result = Vec::new();
        let mut deletes: Vec<(usize, Trit)> = Vec::new();
        let mut inserts: Vec<(usize, Trit)> = Vec::new();

        let flush = |deletes: &mut Vec<(usize, Trit)>,
                     inserts: &mut Vec<(usize, Trit)>,
                     result: &mut Vec<DiffOp>| {
            // Pair as many deletes with inserts as possible into Change ops.
            let pairs = deletes.len().min(inserts.len());
            for k in 0..pairs {
                let (d_pos, d_trit) = deletes[k];
                let (_, i_trit) = inserts[k];
                result.push(DiffOp::Change {
                    pos: d_pos,
                    old: d_trit,
                    new: i_trit,
                });
            }
            for &(pos, trit) in deletes.iter().skip(pairs) {
                result.push(DiffOp::Delete { pos, trit });
            }
            for &(pos, trit) in inserts.iter().skip(pairs) {
                result.push(DiffOp::Insert { pos, trit });
            }
            deletes.clear();
            inserts.clear();
        };

        for op in ops {
            match op {
                DiffOp::Delete { pos, trit } => deletes.push((*pos, *trit)),
                DiffOp::Insert { pos, trit } => inserts.push((*pos, *trit)),
                _ => {
                    flush(&mut deletes, &mut inserts, &mut result);
                    result.push(op.clone());
                }
            }
        }

        // Flush any pending deletes/inserts at the end.
        flush(&mut deletes, &mut inserts, &mut result);

        result
    }

    /// Borrow the ordered list of diff operations.
    pub fn ops(&self) -> &[DiffOp] {
        &self.ops
    }

    /// Count of operations that are *not* `Equal` (i.e. the number of actual
    /// edits).
    pub fn change_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|op| !matches!(op, DiffOp::Equal { .. }))
            .count()
    }

    /// A similarity ratio in `[0.0, 1.0]`: the fraction of operations that are
    /// `Equal`. Two identical (including two empty) sequences score `1.0`.
    pub fn similarity(&self) -> f64 {
        if self.ops.is_empty() {
            return 1.0;
        }
        let equal = self
            .ops
            .iter()
            .filter(|op| matches!(op, DiffOp::Equal { .. }))
            .count();
        equal as f64 / self.ops.len() as f64
    }
}

/// Apply or reverse diffs against [`TernarySeq`] values.
pub struct TernaryPatch;

impl TernaryPatch {
    /// Apply `diff` to `seq`, reconstructing the target sequence.
    ///
    /// The operations are walked in order alongside a cursor into `seq`:
    /// `Equal`, `Change`, and `Delete` each consume exactly one element of
    /// `seq`, while `Insert` emits a new element without advancing the cursor.
    /// Because [`TernaryDiff::diff`] always produces a complete script that
    /// covers every element of the source exactly once, this cursor-based
    /// reconstruction reproduces the target exactly — and is immune to the
    /// position-shift bugs that arithmetic-offset patching suffers from when
    /// deletes and inserts interleave.
    ///
    /// In particular, `apply(a, &diff(a, b)) == b` for any two sequences.
    pub fn apply(seq: &TernarySeq, diff: &TernaryDiff) -> TernarySeq {
        let mut result: Vec<Trit> = Vec::with_capacity(seq.len());
        let mut oi = 0usize; // cursor into `seq`

        for op in &diff.ops {
            match op {
                DiffOp::Equal { trit, .. } => {
                    // Retain the matched source element as-is.
                    if oi < seq.trits.len() {
                        result.push(seq.trits[oi]);
                    } else {
                        // Defensive: a malformed diff claiming more equal
                        // elements than the source holds. Emit the recorded
                        // value so the length contract still holds.
                        result.push(*trit);
                    }
                    oi += 1;
                }
                DiffOp::Change { new, .. } => {
                    // Replace the consumed source element with the new value.
                    result.push(*new);
                    oi += 1;
                }
                DiffOp::Delete { .. } => {
                    // Drop the consumed source element.
                    oi += 1;
                }
                DiffOp::Insert { trit, .. } => {
                    // Emit a new element without advancing the source cursor.
                    result.push(*trit);
                }
            }
        }

        TernarySeq::new(result)
    }

    /// Produce the inverse of a diff: swap `Insert` ↔ `Delete` and swap
    /// `old`/`new` in `Change` operations.
    ///
    /// Applying the reversed diff to the target recovers the original:
    /// `apply(b, &reverse(diff(a, b))) == a`.
    pub fn reverse(diff: &TernaryDiff) -> TernaryDiff {
        let reversed_ops = diff
            .ops
            .iter()
            .map(|op| match op {
                DiffOp::Equal { pos, trit } => DiffOp::Equal {
                    pos: *pos,
                    trit: *trit,
                },
                DiffOp::Change { pos, old, new } => DiffOp::Change {
                    pos: *pos,
                    old: *new,
                    new: *old,
                },
                DiffOp::Insert { pos, trit } => DiffOp::Delete {
                    pos: *pos,
                    trit: *trit,
                },
                DiffOp::Delete { pos, trit } => DiffOp::Insert {
                    pos: *pos,
                    trit: *trit,
                },
            })
            .collect();
        TernaryDiff { ops: reversed_ops }
    }
}

/// A conflict discovered during a three-way merge.
#[derive(Debug, Clone, PartialEq)]
pub enum Conflict {
    /// Both sides changed the same position to different values.
    ChangeConflict {
        /// Position that conflicts.
        pos: usize,
        /// The base (common-ancestor) value.
        base: Trit,
        /// The left branch's value.
        left: Trit,
        /// The right branch's value.
        right: Trit,
    },
    /// One side deleted the element while the other changed it.
    DeleteChangeConflict {
        /// Position that conflicts.
        pos: usize,
        /// The value that was deleted.
        deleted: Trit,
        /// The value the other side changed it to.
        changed_to: Trit,
        /// `true` if the left branch was the one that *changed* (and right
        /// deleted); `false` if right changed and left deleted.
        changer_left: bool,
    },
}

/// The outcome of a [`ThreeWayMerge`]: the merged sequence plus any conflicts.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// The merged sequence. At conflicting positions the left value is taken
    /// by default, pending resolution.
    pub merged: TernarySeq,
    /// Conflicts found during the merge, in position order.
    pub conflicts: Vec<Conflict>,
    /// Convenience flag: `true` iff `conflicts` is non-empty.
    pub has_conflicts: bool,
}

/// Position-wise three-way merge of two sequences against a common base.
///
/// At each index the base, left, and right values are compared: if only one
/// side changed it is taken; if both changed identically it is taken;
/// otherwise it is a conflict. Note that this is *position-based* (see the
/// crate-level docs and the "Known Limitations" in the README): it does not
/// align sequences first, so an insertion on one side shifts every later
/// position.
pub struct ThreeWayMerge;

impl ThreeWayMerge {
    /// Merge `left` and `right` against their common `base`, returning the
    /// merged sequence and any conflicts.
    pub fn merge(base: &TernarySeq, left: &TernarySeq, right: &TernarySeq) -> MergeResult {
        let max_len = base.len().max(left.len()).max(right.len());
        let mut merged = Vec::with_capacity(max_len);
        let mut conflicts = Vec::new();

        for i in 0..max_len {
            let b = base.get(i);
            let l = left.get(i);
            let r = right.get(i);

            match (b, l, r) {
                // All three identical.
                (Some(bv), Some(lv), Some(rv)) if bv == lv && lv == rv => {
                    merged.push(bv);
                }
                // Only the left branch changed.
                (Some(bv), Some(lv), Some(rv)) if lv != bv && rv == bv => {
                    merged.push(lv);
                }
                // Only the right branch changed.
                (Some(bv), Some(lv), Some(rv)) if rv != bv && lv == bv => {
                    merged.push(rv);
                }
                // Both branches changed to the same value.
                (Some(_bv), Some(lv), Some(rv)) if lv == rv => {
                    merged.push(lv);
                }
                // Both changed differently — conflict.
                (Some(bv), Some(lv), Some(rv)) => {
                    conflicts.push(Conflict::ChangeConflict {
                        pos: i,
                        base: bv,
                        left: lv,
                        right: rv,
                    });
                    merged.push(lv); // Default: take left pending resolution.
                }
                // Base absent (shorter) but both branches present here.
                (None, Some(lv), Some(rv)) => {
                    if lv == rv {
                        merged.push(lv);
                    } else {
                        conflicts.push(Conflict::ChangeConflict {
                            pos: i,
                            base: Trit::Zero,
                            left: lv,
                            right: rv,
                        });
                        merged.push(lv);
                    }
                }
                (None, Some(lv), None) => merged.push(lv),
                (None, None, Some(rv)) => merged.push(rv),
                (None, None, None) => {}
                // Left deleted, right unchanged — delete.
                (Some(bv), None, Some(rv)) if rv == bv => {}
                // Left deleted, right changed — conflict.
                (Some(bv), None, Some(rv)) => {
                    conflicts.push(Conflict::DeleteChangeConflict {
                        pos: i,
                        deleted: bv,
                        changed_to: rv,
                        changer_left: false,
                    });
                }
                // Right deleted, left unchanged — delete.
                (Some(bv), Some(lv), None) if lv == bv => {}
                // Right deleted, left changed — conflict.
                (Some(bv), Some(lv), None) => {
                    conflicts.push(Conflict::DeleteChangeConflict {
                        pos: i,
                        deleted: bv,
                        changed_to: lv,
                        changer_left: true,
                    });
                }
                // Both deleted — agree, nothing to emit.
                (Some(_bv), None, None) => {}
            }
        }

        MergeResult {
            has_conflicts: !conflicts.is_empty(),
            merged: TernarySeq::new(merged),
            conflicts,
        }
    }
}

/// Strategy for resolving merge conflicts.
#[derive(Debug, Clone, Copy)]
pub enum ResolutionStrategy {
    /// Take the left branch's value.
    TakeLeft,
    /// Take the right branch's value.
    TakeRight,
    /// Take the base (common-ancestor) value.
    TakeBase,
    /// Force the neutral value (`Trit::Zero`).
    Neutral,
    /// Take the higher ternary value (`to_i8` maximum).
    Max,
    /// Take the lower ternary value (`to_i8` minimum).
    Min,
}

/// Resolve every conflict in a [`MergeResult`] according to a single strategy.
pub struct ConflictResolver;

impl ConflictResolver {
    /// Resolve all conflicts in `result` using `strategy`, clearing the
    /// conflict list afterwards.
    pub fn resolve(result: &mut MergeResult, strategy: ResolutionStrategy) {
        for conflict in &result.conflicts {
            match conflict {
                Conflict::ChangeConflict {
                    pos,
                    base,
                    left,
                    right,
                } => {
                    let resolved = match strategy {
                        ResolutionStrategy::TakeLeft => *left,
                        ResolutionStrategy::TakeRight => *right,
                        ResolutionStrategy::TakeBase => *base,
                        ResolutionStrategy::Neutral => Trit::Zero,
                        ResolutionStrategy::Max => {
                            let v = left.to_i8().max(right.to_i8());
                            Trit::from_i8(v).unwrap_or(Trit::Zero)
                        }
                        ResolutionStrategy::Min => {
                            let v = left.to_i8().min(right.to_i8());
                            Trit::from_i8(v).unwrap_or(Trit::Zero)
                        }
                    };
                    if *pos < result.merged.len() {
                        result.merged.set(*pos, resolved);
                    }
                }
                Conflict::DeleteChangeConflict {
                    pos,
                    changed_to,
                    changer_left,
                    ..
                } => {
                    let resolved = match strategy {
                        ResolutionStrategy::TakeLeft => {
                            if *changer_left {
                                *changed_to
                            } else {
                                Trit::Zero
                            }
                        }
                        ResolutionStrategy::TakeRight => {
                            if !*changer_left {
                                *changed_to
                            } else {
                                Trit::Zero
                            }
                        }
                        ResolutionStrategy::TakeBase => Trit::Zero,
                        ResolutionStrategy::Neutral => Trit::Zero,
                        ResolutionStrategy::Max => {
                            let v = changed_to.to_i8().max(0);
                            Trit::from_i8(v).unwrap_or(Trit::Zero)
                        }
                        ResolutionStrategy::Min => {
                            let v = changed_to.to_i8().min(0);
                            Trit::from_i8(v).unwrap_or(Trit::Zero)
                        }
                    };
                    if *pos < result.merged.len() {
                        result.merged.set(*pos, resolved);
                    }
                }
            }
        }
        result.conflicts.clear();
        result.has_conflicts = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Trit ---

    #[test]
    fn test_trit_basic() {
        assert_eq!(Trit::Neg.to_i8(), -1);
        assert_eq!(Trit::Zero.to_i8(), 0);
        assert_eq!(Trit::Pos.to_i8(), 1);
    }

    #[test]
    fn test_trit_roundtrip() {
        for v in [-1, 0, 1] {
            assert_eq!(Trit::from_i8(v).unwrap().to_i8(), v);
        }
        assert_eq!(Trit::from_i8(2), None);
        assert_eq!(Trit::from_i8(-2), None);
        assert_eq!(Trit::from_i8(127), None);
    }

    // --- TernarySeq ---

    #[test]
    fn test_seq_basic() {
        let seq = TernarySeq::from_i8(&[-1, 0, 1]);
        assert_eq!(seq.len(), 3);
        assert_eq!(seq.get(0), Some(Trit::Neg));
        assert_eq!(seq.get(2), Some(Trit::Pos));
        assert_eq!(seq.get(3), None);
        assert!(!seq.is_empty());
        assert!(TernarySeq::new(vec![]).is_empty());
    }

    #[test]
    fn test_seq_push() {
        let mut seq = TernarySeq::new(vec![]);
        seq.push(Trit::Pos);
        seq.push(Trit::Neg);
        assert_eq!(seq.len(), 2);
        assert_eq!(seq.trits(), &[Trit::Pos, Trit::Neg]);
    }

    #[test]
    fn test_seq_insert_remove() {
        let mut seq = TernarySeq::from_i8(&[0, 0, 0]);
        seq.insert(1, Trit::Pos);
        assert_eq!(seq.len(), 4);
        assert_eq!(seq.get(1), Some(Trit::Pos));
        assert_eq!(seq.remove(1), Some(Trit::Pos));
        assert_eq!(seq.len(), 3);
        assert_eq!(seq.remove(99), None);
    }

    #[test]
    fn test_seq_slice() {
        let seq = TernarySeq::from_i8(&[-1, 0, 1, -1, 0]);
        let slice = seq.slice(1, 4);
        assert_eq!(slice.len(), 3);
        assert_eq!(slice.get(0), Some(Trit::Zero));
    }

    #[test]
    fn test_seq_slice_out_of_range_never_panics() {
        // All of these must not panic; bounds are clamped.
        let seq = TernarySeq::from_i8(&[-1, 0, 1]);
        assert_eq!(seq.slice(0, 99).len(), 3); // end clamped to len
        assert_eq!(seq.slice(99, 99).len(), 0); // start clamped to end
        assert_eq!(seq.slice(99, 1).len(), 0); // start > end -> empty
        assert_eq!(seq.slice(1, 1).len(), 0); // empty range
        assert_eq!(seq.slice(2, 5).trits(), &[Trit::Pos]);
    }

    #[test]
    fn test_seq_set_out_of_bounds_is_noop() {
        let mut seq = TernarySeq::from_i8(&[0, 0]);
        seq.set(5, Trit::Pos); // out of bounds: silent no-op
        assert_eq!(seq.trits(), &[Trit::Zero, Trit::Zero]);
        seq.set(0, Trit::Pos);
        assert_eq!(seq.get(0), Some(Trit::Pos));
    }

    #[test]
    fn test_from_i8_silent_drop_vs_strict() {
        // from_i8 silently drops invalid values (documented behavior).
        let dropped = TernarySeq::from_i8(&[1, 2, 3]);
        assert_eq!(dropped.trits(), &[Trit::Pos]);
        // from_i8_strict surfaces the first invalid value.
        assert_eq!(TernarySeq::from_i8_strict(&[1, 2, 3]), Err(2));
        assert!(TernarySeq::from_i8_strict(&[-1, 0, 1]).is_ok());
        assert!(TernarySeq::from_i8_strict(&[]).unwrap().is_empty());
    }

    // --- Diff ---

    #[test]
    fn test_diff_equal() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, -1, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.change_count(), 0);
        assert_eq!(diff.ops().len(), 3);
        assert!(diff
            .ops()
            .iter()
            .all(|op| matches!(op, DiffOp::Equal { .. })));
    }

    #[test]
    fn test_diff_change() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, 0, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        // Exactly one position changes value (-1 -> 0).
        assert_eq!(diff.change_count(), 1);
        assert_eq!(
            diff.ops(),
            &[
                DiffOp::Equal {
                    pos: 0,
                    trit: Trit::Pos
                },
                DiffOp::Change {
                    pos: 1,
                    old: Trit::Neg,
                    new: Trit::Zero
                },
                DiffOp::Equal {
                    pos: 2,
                    trit: Trit::Zero
                },
            ]
        );
    }

    #[test]
    fn test_diff_insert() {
        let a = TernarySeq::from_i8(&[1, 0]);
        let b = TernarySeq::from_i8(&[1, -1, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        // Exactly one insertion.
        assert_eq!(
            diff.ops()
                .iter()
                .filter(|o| matches!(o, DiffOp::Insert { .. }))
                .count(),
            1
        );
        assert_eq!(diff.change_count(), 1);
        // Round-trips.
        assert_eq!(TernaryPatch::apply(&a, &diff).trits(), b.trits());
    }

    #[test]
    fn test_diff_delete() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        // Exactly one deletion.
        assert_eq!(
            diff.ops()
                .iter()
                .filter(|o| matches!(o, DiffOp::Delete { .. }))
                .count(),
            1
        );
        assert_eq!(diff.change_count(), 1);
        assert_eq!(TernaryPatch::apply(&a, &diff).trits(), b.trits());
    }

    #[test]
    fn test_diff_empty() {
        let a = TernarySeq::new(vec![]);
        let b = TernarySeq::new(vec![]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.ops().len(), 0);
        assert_eq!(diff.change_count(), 0);
        assert_eq!(diff.similarity(), 1.0);
        assert_eq!(TernaryPatch::apply(&a, &diff).trits(), b.trits());
    }

    #[test]
    fn test_diff_insert_all() {
        let a = TernarySeq::new(vec![]);
        let b = TernarySeq::from_i8(&[1, -1, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.change_count(), 3);
        assert!(diff
            .ops()
            .iter()
            .all(|op| matches!(op, DiffOp::Insert { .. })));
        assert_eq!(TernaryPatch::apply(&a, &diff).trits(), b.trits());
    }

    #[test]
    fn test_diff_delete_all() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::new(vec![]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.change_count(), 3);
        assert!(diff
            .ops()
            .iter()
            .all(|op| matches!(op, DiffOp::Delete { .. })));
        assert_eq!(TernaryPatch::apply(&a, &diff).trits(), b.trits());
    }

    #[test]
    fn test_diff_disjoint() {
        // Completely disjoint: no common element, LCS length 0.
        let a = TernarySeq::from_i8(&[1, 1, 1]);
        let b = TernarySeq::from_i8(&[-1, -1, -1]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.similarity(), 0.0);
        assert_eq!(diff.change_count(), 3);
        assert_eq!(TernaryPatch::apply(&a, &diff).trits(), b.trits());
    }

    #[test]
    fn test_diff_single_element() {
        // Single element, changed.
        let a = TernarySeq::from_i8(&[0]);
        let b = TernarySeq::from_i8(&[1]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.ops().len(), 1);
        assert_eq!(diff.change_count(), 1);
        assert_eq!(TernaryPatch::apply(&a, &diff).trits(), b.trits());
        // Single element, identical.
        let d2 = TernaryDiff::diff(&a, &a);
        assert_eq!(d2.change_count(), 0);
    }

    // --- Patch round-trip (the core invariant) ---

    /// Property-style helper: diff then patch must reconstruct b exactly, and
    /// the reversed diff applied to b must reconstruct a exactly.
    fn assert_roundtrip(a: &TernarySeq, b: &TernarySeq) {
        let diff = TernaryDiff::diff(a, b);
        let patched = TernaryPatch::apply(a, &diff);
        assert_eq!(patched.trits(), b.trits(), "forward round-trip failed");

        let reversed = TernaryPatch::reverse(&diff);
        let restored = TernaryPatch::apply(b, &reversed);
        assert_eq!(restored.trits(), a.trits(), "reverse round-trip failed");
    }

    #[test]
    fn test_patch_apply() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, 0, 0]);
        assert_roundtrip(&a, &b);
    }

    #[test]
    fn test_patch_reverse() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, 0, 0]);
        assert_roundtrip(&a, &b);
    }

    /// The exact scenario from the README Quick Start. Previously this FAILED:
    /// the offset-based apply produced [1,0,-1,0] instead of [1,0,0,-1] because
    /// a Change (built from a delete at old index 3) and a leftover Insert
    /// (new index 3) desynced the offset bookkeeping.
    #[test]
    fn test_patch_roundtrip_readme_example() {
        let a = TernarySeq::from_i8(&[1, -1, 0, 1]);
        let b = TernarySeq::from_i8(&[1, 0, 0, -1]);
        assert_roundtrip(&a, &b);
        // Sanity: the diff really is non-trivial (mixes a change and an insert).
        let diff = TernaryDiff::diff(&a, &b);
        assert!(diff.change_count() >= 2);
    }

    #[test]
    fn test_patch_roundtrip_many_cases() {
        // A spread of shapes that stress the patch cursor: inserts, deletes,
        // changes, mismatched lengths, empty ends, and interleaving.
        let cases: &[(&[i8], &[i8])] = &[
            (&[1, -1, 0, 1], &[1, 0, 0, -1]),           // README example
            (&[1, 1, 1], &[-1, -1, -1]),                // disjoint
            (&[1, 0], &[1, -1, 0]),                     // pure insert
            (&[1, -1, 0], &[1, 0]),                     // pure delete
            (&[], &[1, -1, 0]),                         // all insert
            (&[1, -1, 0], &[]),                         // all delete
            (&[1, 0, 1, 0], &[1, 1]),                   // mismatched lengths
            (&[0], &[1]),                               // single change
            (&[0], &[0]),                               // single equal
            (&[], &[]),                                 // both empty
            (&[1, -1, 1, -1, 1], &[1, 0, 0, -1, 0, 1]), // mixed, lengthen
            (&[1, 0, 1, 0, 1, -1], &[0, 1, -1]),        // mixed, shorten
        ];
        for (a, b) in cases {
            let sa = TernarySeq::from_i8(a);
            let sb = TernarySeq::from_i8(b);
            assert_roundtrip(&sa, &sb);
        }
    }

    // --- Similarity ---

    #[test]
    fn test_similarity_identical() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let diff = TernaryDiff::diff(&a, &a);
        assert_eq!(diff.similarity(), 1.0);
    }

    #[test]
    fn test_similarity_different() {
        // Fully disjoint: similarity is exactly 0.0, not merely "< 0.5".
        let a = TernarySeq::from_i8(&[1, 1, 1]);
        let b = TernarySeq::from_i8(&[-1, -1, -1]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.similarity(), 0.0);
    }

    // --- Three-way merge ---

    #[test]
    fn test_three_way_merge_no_conflict() {
        let base = TernarySeq::from_i8(&[0, 0, 0]);
        let left = TernarySeq::from_i8(&[1, 0, 0]);
        let right = TernarySeq::from_i8(&[0, 0, -1]);
        let result = ThreeWayMerge::merge(&base, &left, &right);
        assert!(!result.has_conflicts);
        assert_eq!(result.merged.get(0), Some(Trit::Pos));
        assert_eq!(result.merged.get(2), Some(Trit::Neg));
    }

    #[test]
    fn test_three_way_merge_conflict() {
        let base = TernarySeq::from_i8(&[0, 0, 0]);
        let left = TernarySeq::from_i8(&[1, 0, 0]);
        let right = TernarySeq::from_i8(&[-1, 0, 0]);
        let result = ThreeWayMerge::merge(&base, &left, &right);
        assert!(result.has_conflicts);
        assert_eq!(result.conflicts.len(), 1);
    }

    #[test]
    fn test_three_way_merge_delete_change_conflict() {
        // Base has 3 elements; left deletes index 0, right changes index 0.
        let base = TernarySeq::from_i8(&[0, 0, 0]);
        let left = TernarySeq::from_i8(&[0, 0]); // shorter -> "deleted" index 2
        let right = TernarySeq::from_i8(&[0, 0, 1]); // changed index 2
        let result = ThreeWayMerge::merge(&base, &left, &right);
        assert!(result.has_conflicts);
        assert!(result
            .conflicts
            .iter()
            .any(|c| matches!(c, Conflict::DeleteChangeConflict { .. })));
    }

    // --- Conflict resolution ---

    #[test]
    fn test_conflict_resolver_left() {
        let base = TernarySeq::from_i8(&[0, 0, 0]);
        let left = TernarySeq::from_i8(&[1, 0, 0]);
        let right = TernarySeq::from_i8(&[-1, 0, 0]);
        let mut result = ThreeWayMerge::merge(&base, &left, &right);
        ConflictResolver::resolve(&mut result, ResolutionStrategy::TakeLeft);
        assert!(!result.has_conflicts);
        assert_eq!(result.merged.get(0), Some(Trit::Pos));
    }

    #[test]
    fn test_conflict_resolver_right() {
        let base = TernarySeq::from_i8(&[0, 0, 0]);
        let left = TernarySeq::from_i8(&[1, 0, 0]);
        let right = TernarySeq::from_i8(&[-1, 0, 0]);
        let mut result = ThreeWayMerge::merge(&base, &left, &right);
        ConflictResolver::resolve(&mut result, ResolutionStrategy::TakeRight);
        assert!(!result.has_conflicts);
        assert_eq!(result.merged.get(0), Some(Trit::Neg));
    }

    #[test]
    fn test_conflict_resolver_neutral() {
        let base = TernarySeq::from_i8(&[0, 0, 0]);
        let left = TernarySeq::from_i8(&[1, 0, 0]);
        let right = TernarySeq::from_i8(&[-1, 0, 0]);
        let mut result = ThreeWayMerge::merge(&base, &left, &right);
        ConflictResolver::resolve(&mut result, ResolutionStrategy::Neutral);
        assert_eq!(result.merged.get(0), Some(Trit::Zero));
    }

    #[test]
    fn test_conflict_resolver_max_min() {
        let base = TernarySeq::from_i8(&[0, 0]);
        let left = TernarySeq::from_i8(&[1, -1]);
        let right = TernarySeq::from_i8(&[-1, 1]);
        // Both positions conflict: {+1,-1} and {-1,+1}.
        let mut mx = ThreeWayMerge::merge(&base, &left, &right);
        ConflictResolver::resolve(&mut mx, ResolutionStrategy::Max);
        assert_eq!(mx.merged.trits(), &[Trit::Pos, Trit::Pos]);

        let mut mn = ThreeWayMerge::merge(&base, &left, &right);
        ConflictResolver::resolve(&mut mn, ResolutionStrategy::Min);
        assert_eq!(mn.merged.trits(), &[Trit::Neg, Trit::Neg]);
    }
}
