//! # ternary-diff
//!
//! Diff and patch for ternary strategies: compare, merge, and resolve conflicts.
//!
//! Provides:
//! - `Trit` — Core ternary value
//! - `TernaryDiff` — Diff computation between ternary sequences
//! - `TernaryPatch` — Apply patches to sequences
//! - `ThreeWayMerge` — Three-way merge with conflict detection
//! - `ConflictResolver` — Strategies for resolving merge conflicts

/// Ternary value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trit {
    Neg,
    Zero,
    Pos,
}

impl Trit {
    pub fn to_i8(self) -> i8 {
        match self {
            Trit::Neg => -1,
            Trit::Zero => 0,
            Trit::Pos => 1,
        }
    }

    pub fn from_i8(v: i8) -> Option<Self> {
        match v {
            -1 => Some(Trit::Neg),
            0 => Some(Trit::Zero),
            1 => Some(Trit::Pos),
            _ => None,
        }
    }
}

/// A sequence of ternary values
#[derive(Debug, Clone, PartialEq)]
pub struct TernarySeq {
    trits: Vec<Trit>,
}

impl TernarySeq {
    pub fn new(trits: Vec<Trit>) -> Self {
        TernarySeq { trits }
    }

    pub fn from_i8(values: &[i8]) -> Self {
        TernarySeq {
            trits: values.iter().filter_map(|&v| Trit::from_i8(v)).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.trits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.trits.is_empty()
    }

    pub fn trits(&self) -> &[Trit] {
        &self.trits
    }

    pub fn get(&self, idx: usize) -> Option<Trit> {
        self.trits.get(idx).copied()
    }

    pub fn set(&mut self, idx: usize, trit: Trit) {
        if idx < self.trits.len() {
            self.trits[idx] = trit;
        }
    }

    pub fn push(&mut self, trit: Trit) {
        self.trits.push(trit);
    }

    pub fn insert(&mut self, idx: usize, trit: Trit) {
        self.trits.insert(idx, trit);
    }

    pub fn remove(&mut self, idx: usize) -> Option<Trit> {
        if idx < self.trits.len() {
            Some(self.trits.remove(idx))
        } else {
            None
        }
    }

    /// Slice of the sequence
    pub fn slice(&self, start: usize, end: usize) -> TernarySeq {
        TernarySeq::new(self.trits[start..end.min(self.trits.len())].to_vec())
    }
}

/// A single diff operation
#[derive(Debug, Clone, PartialEq)]
pub enum DiffOp {
    /// No change at position
    Equal { pos: usize, trit: Trit },
    /// Value changed at position
    Change { pos: usize, old: Trit, new: Trit },
    /// Value inserted at position
    Insert { pos: usize, trit: Trit },
    /// Value removed at position
    Delete { pos: usize, trit: Trit },
}

/// Result of a diff operation
#[derive(Debug, Clone)]
pub struct TernaryDiff {
    ops: Vec<DiffOp>,
}

impl TernaryDiff {
    /// Compute diff between two sequences using LCS-based algorithm
    pub fn diff(old: &TernarySeq, new: &TernarySeq) -> Self {
        let m = old.len();
        let n = new.len();

        // Build LCS table
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

        // Backtrack to find diff
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

        // Convert to changes where appropriate
        let normalized = Self::normalize_ops(&ops, old, new);
        TernaryDiff { ops: normalized }
    }

    fn normalize_ops(ops: &[DiffOp], old: &TernarySeq, _new: &TernarySeq) -> Vec<DiffOp> {
        let mut result = Vec::new();
        let mut deletes: Vec<(usize, Trit)> = Vec::new();
        let mut inserts: Vec<(usize, Trit)> = Vec::new();

        for op in ops {
            match op {
                DiffOp::Delete { pos, trit } => deletes.push((*pos, *trit)),
                DiffOp::Insert { pos, trit } => inserts.push((*pos, *trit)),
                _ => {
                    // Flush pending deletes and inserts as changes
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
                    for k in pairs..deletes.len() {
                        let (pos, trit) = deletes[k];
                        result.push(DiffOp::Delete { pos, trit });
                    }
                    for k in pairs..inserts.len() {
                        let (pos, trit) = inserts[k];
                        result.push(DiffOp::Insert { pos, trit });
                    }
                    deletes.clear();
                    inserts.clear();
                    result.push(op.clone());
                }
            }
        }

        // Flush remaining
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
        for k in pairs..deletes.len() {
            let (pos, trit) = deletes[k];
            result.push(DiffOp::Delete { pos, trit });
        }
        for k in pairs..inserts.len() {
            let (pos, trit) = inserts[k];
            result.push(DiffOp::Insert { pos, trit });
        }

        result
    }

    pub fn ops(&self) -> &[DiffOp] {
        &self.ops
    }

    /// Count of changes
    pub fn change_count(&self) -> usize {
        self.ops.iter().filter(|op| !matches!(op, DiffOp::Equal { .. })).count()
    }

    /// Calculate similarity (0.0 to 1.0)
    pub fn similarity(&self) -> f64 {
        if self.ops.is_empty() {
            return 1.0;
        }
        let equal = self.ops.iter().filter(|op| matches!(op, DiffOp::Equal { .. })).count();
        equal as f64 / self.ops.len() as f64
    }
}

/// Patch application
pub struct TernaryPatch;

impl TernaryPatch {
    /// Apply a diff to a sequence
    pub fn apply(seq: &TernarySeq, diff: &TernaryDiff) -> TernarySeq {
        let mut result = seq.clone();
        let mut offset = 0i64;

        for op in &diff.ops {
            match op {
                DiffOp::Equal { .. } => {}
                DiffOp::Change { pos, new, .. } => {
                    let adjusted = (*pos as i64 + offset) as usize;
                    result.set(adjusted, *new);
                }
                DiffOp::Insert { pos, trit } => {
                    let adjusted = (*pos as i64 + offset).min(result.len() as i64) as usize;
                    if adjusted <= result.len() {
                        result.trits.insert(adjusted, *trit);
                        offset += 1;
                    }
                }
                DiffOp::Delete { pos, .. } => {
                    let adjusted = (*pos as i64 + offset) as usize;
                    if adjusted < result.len() {
                        result.trits.remove(adjusted);
                        offset -= 1;
                    }
                }
            }
        }
        result
    }

    /// Reverse a diff
    pub fn reverse(diff: &TernaryDiff) -> TernaryDiff {
        let reversed_ops = diff.ops.iter().map(|op| match op {
            DiffOp::Equal { pos, trit } => DiffOp::Equal { pos: *pos, trit: *trit },
            DiffOp::Change { pos, old, new } => DiffOp::Change { pos: *pos, old: *new, new: *old },
            DiffOp::Insert { pos, trit } => DiffOp::Delete { pos: *pos, trit: *trit },
            DiffOp::Delete { pos, trit } => DiffOp::Insert { pos: *pos, trit: *trit },
        }).collect();
        TernaryDiff { ops: reversed_ops }
    }
}

/// Merge conflict
#[derive(Debug, Clone, PartialEq)]
pub enum Conflict {
    /// Both sides changed the same position differently
    ChangeConflict {
        pos: usize,
        base: Trit,
        left: Trit,
        right: Trit,
    },
    /// One side deleted, other changed
    DeleteChangeConflict {
        pos: usize,
        deleted: Trit,
        changed_to: Trit,
        changer_left: bool,
    },
}

/// Three-way merge result
#[derive(Debug, Clone)]
pub struct MergeResult {
    pub merged: TernarySeq,
    pub conflicts: Vec<Conflict>,
    pub has_conflicts: bool,
}

/// Three-way merge
pub struct ThreeWayMerge;

impl ThreeWayMerge {
    /// Merge two sequences against a common base
    pub fn merge(base: &TernarySeq, left: &TernarySeq, right: &TernarySeq) -> MergeResult {
        let max_len = base.len().max(left.len()).max(right.len());
        let mut merged = Vec::with_capacity(max_len);
        let mut conflicts = Vec::new();

        for i in 0..max_len {
            let b = base.get(i);
            let l = left.get(i);
            let r = right.get(i);

            match (b, l, r) {
                // All same
                (Some(bv), Some(lv), Some(rv)) if bv == lv && lv == rv => {
                    merged.push(bv);
                }
                // Only left changed
                (Some(bv), Some(lv), Some(rv)) if lv != bv && rv == bv => {
                    merged.push(lv);
                }
                // Only right changed
                (Some(bv), Some(lv), Some(rv)) if rv != bv && lv == bv => {
                    merged.push(rv);
                }
                // Both changed to same
                (Some(_bv), Some(lv), Some(rv)) if lv == rv => {
                    merged.push(lv);
                }
                // Both changed differently — conflict
                (Some(bv), Some(lv), Some(rv)) => {
                    conflicts.push(Conflict::ChangeConflict {
                        pos: i,
                        base: bv,
                        left: lv,
                        right: rv,
                    });
                    merged.push(lv); // Default: take left
                }
                // Base deleted, left and/or right changed
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
                // One side deleted
                (Some(bv), None, Some(rv)) if rv == bv => {
                    // Left deleted, right unchanged — delete
                }
                (Some(bv), None, Some(rv)) => {
                    conflicts.push(Conflict::DeleteChangeConflict {
                        pos: i,
                        deleted: bv,
                        changed_to: rv,
                        changer_left: false,
                    });
                }
                (Some(bv), Some(lv), None) if lv == bv => {
                    // Right deleted, left unchanged — delete
                }
                (Some(bv), Some(lv), None) => {
                    conflicts.push(Conflict::DeleteChangeConflict {
                        pos: i,
                        deleted: bv,
                        changed_to: lv,
                        changer_left: true,
                    });
                }
                (Some(_bv), None, None) => {
                    // Both deleted — ok
                }
            }
        }

        MergeResult {
            has_conflicts: !conflicts.is_empty(),
            merged: TernarySeq::new(merged),
            conflicts,
        }
    }
}

/// Conflict resolution strategies
#[derive(Debug, Clone, Copy)]
pub enum ResolutionStrategy {
    /// Take the left side
    TakeLeft,
    /// Take the right side
    TakeRight,
    /// Take the base value
    TakeBase,
    /// Use Trit::Zero (neutral)
    Neutral,
    /// Take the max value
    Max,
    /// Take the min value
    Min,
}

pub struct ConflictResolver;

impl ConflictResolver {
    /// Resolve conflicts using a given strategy
    pub fn resolve(result: &mut MergeResult, strategy: ResolutionStrategy) {
        for conflict in &result.conflicts {
            match conflict {
                Conflict::ChangeConflict { pos, base, left, right } => {
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
                Conflict::DeleteChangeConflict { pos, changed_to, changer_left, .. } => {
                    let resolved = match strategy {
                        ResolutionStrategy::TakeLeft => {
                            if *changer_left { *changed_to } else { Trit::Zero }
                        }
                        ResolutionStrategy::TakeRight => {
                            if !changer_left { *changed_to } else { Trit::Zero }
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

    #[test]
    fn test_trit_basic() {
        assert_eq!(Trit::Neg.to_i8(), -1);
        assert_eq!(Trit::Zero.to_i8(), 0);
        assert_eq!(Trit::Pos.to_i8(), 1);
    }

    #[test]
    fn test_seq_basic() {
        let seq = TernarySeq::from_i8(&[-1, 0, 1]);
        assert_eq!(seq.len(), 3);
        assert_eq!(seq.get(0), Some(Trit::Neg));
        assert_eq!(seq.get(2), Some(Trit::Pos));
    }

    #[test]
    fn test_seq_push() {
        let mut seq = TernarySeq::new(vec![]);
        seq.push(Trit::Pos);
        seq.push(Trit::Neg);
        assert_eq!(seq.len(), 2);
    }

    #[test]
    fn test_seq_insert_remove() {
        let mut seq = TernarySeq::from_i8(&[0, 0, 0]);
        seq.insert(1, Trit::Pos);
        assert_eq!(seq.len(), 4);
        assert_eq!(seq.get(1), Some(Trit::Pos));
        seq.remove(1);
        assert_eq!(seq.len(), 3);
    }

    #[test]
    fn test_seq_slice() {
        let seq = TernarySeq::from_i8(&[-1, 0, 1, -1, 0]);
        let slice = seq.slice(1, 4);
        assert_eq!(slice.len(), 3);
        assert_eq!(slice.get(0), Some(Trit::Zero));
    }

    #[test]
    fn test_diff_equal() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, -1, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.change_count(), 0);
        assert_eq!(diff.ops().len(), 3);
    }

    #[test]
    fn test_diff_change() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, 0, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        assert!(diff.change_count() > 0);
    }

    #[test]
    fn test_diff_insert() {
        let a = TernarySeq::from_i8(&[1, 0]);
        let b = TernarySeq::from_i8(&[1, -1, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        assert!(diff.ops().iter().any(|op| matches!(op, DiffOp::Insert { .. })));
    }

    #[test]
    fn test_diff_delete() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        assert!(diff.ops().iter().any(|op| matches!(op, DiffOp::Delete { .. })));
    }

    #[test]
    fn test_patch_apply() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, 0, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        let patched = TernaryPatch::apply(&a, &diff);
        assert_eq!(patched.trits(), b.trits());
    }

    #[test]
    fn test_patch_reverse() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let b = TernarySeq::from_i8(&[1, 0, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        let reversed = TernaryPatch::reverse(&diff);
        let restored = TernaryPatch::apply(&b, &reversed);
        assert_eq!(restored.trits(), a.trits());
    }

    #[test]
    fn test_similarity_identical() {
        let a = TernarySeq::from_i8(&[1, -1, 0]);
        let diff = TernaryDiff::diff(&a, &a);
        assert!((diff.similarity() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_similarity_different() {
        let a = TernarySeq::from_i8(&[1, 1, 1]);
        let b = TernarySeq::from_i8(&[-1, -1, -1]);
        let diff = TernaryDiff::diff(&a, &b);
        assert!(diff.similarity() < 0.5);
    }

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
    }

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
    fn test_diff_empty() {
        let a = TernarySeq::new(vec![]);
        let b = TernarySeq::new(vec![]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.ops().len(), 0);
    }

    #[test]
    fn test_diff_insert_all() {
        let a = TernarySeq::new(vec![]);
        let b = TernarySeq::from_i8(&[1, -1, 0]);
        let diff = TernaryDiff::diff(&a, &b);
        assert_eq!(diff.change_count(), 3);
    }
}
