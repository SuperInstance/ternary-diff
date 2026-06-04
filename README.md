# ternary-diff

Diff, patch, and three-way merge for ternary sequences — compare, transform, and resolve conflicts in {-1, 0, +1} data.

## Why This Exists

Version control works on text lines. But if your data is ternary sequences — sensor streams, voting tallies, cell tissue states — you need diff and merge at the element level. This crate implements LCS-based diffing between ternary sequences, patch application with offset tracking, patch reversal, three-way merge with conflict detection, and six conflict resolution strategies. It's `git diff` for ternary data.

## Core Concepts

- **Trit** — A single ternary value: Neg (−1), Zero (0), Pos (+1).
- **TernarySeq** — An ordered sequence of trits. Supports push, insert, remove, slice, and get/set by index.
- **DiffOp** — A single diff operation: Equal (unchanged at position), Change (value replaced), Insert (new value added), Delete (value removed).
- **TernaryDiff** — A sequence of DiffOps computed via LCS (Longest Common Subsequence). The LCS algorithm finds the longest subsequence shared between old and new, then expresses everything else as changes, inserts, or deletes.
- **TernaryPatch** — Applies a diff to a sequence, tracking position offsets so insertions and deletions don't desync subsequent operations. Also reverses diffs (swap Insert↔Delete, swap old↔new in Change).
- **Three-way merge** — Given a base sequence and two divergent branches (left and right), merge them position-by-position. If only one side changed, take that change. If both changed the same way, take it. If both changed differently, that's a conflict.
- **Conflict** — Either a ChangeConflict (both sides changed the same position to different values) or a DeleteChangeConflict (one side deleted, the other changed).
- **ResolutionStrategy** — TakeLeft, TakeRight, TakeBase, Neutral (force Zero), Max (higher ternary value), Min (lower ternary value).

## Quick Start

```toml
# Cargo.toml
[dependencies]
ternary-diff = "0.1"
```

```rust
use ternary_diff::*;

fn main() {
    // Diff two sequences
    let a = TernarySeq::from_i8(&[1, -1, 0, 1]);
    let b = TernarySeq::from_i8(&[1, 0, 0, -1]);
    let diff = TernaryDiff::diff(&a, &b);
    println!("Changes: {}, Similarity: {:.2}", diff.change_count(), diff.similarity());

    // Apply patch
    let patched = TernaryPatch::apply(&a, &diff);
    assert_eq!(patched.trits(), b.trits());

    // Reverse patch
    let reversed = TernaryPatch::reverse(&diff);
    let restored = TernaryPatch::apply(&b, &reversed);
    assert_eq!(restored.trits(), a.trits());

    // Three-way merge
    let base = TernarySeq::from_i8(&[0, 0, 0]);
    let left = TernarySeq::from_i8(&[1, 0, 0]);
    let right = TernarySeq::from_i8(&[0, 0, -1]);
    let result = ThreeWayMerge::merge(&base, &left, &right);
    println!("Merged: {:?}, Conflicts: {}", result.merged.trits(), result.has_conflicts);

    // Resolve conflicts
    let mut result = ThreeWayMerge::merge(
        &base,
        &TernarySeq::from_i8(&[1, 0, 0]),
        &TernarySeq::from_i8(&[-1, 0, 0]),
    );
    ConflictResolver::resolve(&mut result, ResolutionStrategy::Neutral);
    assert_eq!(result.merged.get(0), Some(Trit::Zero));
}
```

## API Overview

| Type | Description |
|------|-------------|
| `Trit` | Core value: Neg/Zero/Pos |
| `TernarySeq` | Ordered mutable sequence of trits |
| `DiffOp` | Single diff operation (Equal/Change/Insert/Delete) |
| `TernaryDiff` | Computed diff with change count and similarity |
| `TernaryPatch` | Apply or reverse diffs |
| `Conflict` | ChangeConflict or DeleteChangeConflict |
| `MergeResult` | Merged sequence + conflict list |
| `ThreeWayMerge` | Position-wise three-way merge |
| `ResolutionStrategy` | TakeLeft/Right/Base/Neutral/Max/Min |
| `ConflictResolver` | Applies a strategy to resolve all conflicts |

## How It Works

**Diff computation** uses the standard LCS (Longest Common Subsequence) dynamic programming algorithm. An (m+1) × (n+1) table is filled where `dp[i][j]` is the LCS length for the first i elements of old and first j elements of new. Backtracking produces Equal/Insert/Delete operations. A normalization pass pairs adjacent Delete+Insert operations at the same position into Change operations when they represent a value replacement rather than a structural change.

**Patch application** walks the diff operations in order, maintaining an offset counter. Insertions increment the offset; deletions decrement it. Each operation's position is adjusted by the current offset. This ensures correct positioning even when earlier operations change the sequence length.

**Patch reversal** swaps Insert ↔ Delete and swaps old/new in Change operations. Applying the reversed diff to the target sequence recovers the original.

**Three-way merge** operates position-by-position (no alignment algorithm). At each index, it compares base, left, and right values:
- All same → keep it
- Only left changed → take left
- Only right changed → take right
- Both changed to the same → take it
- Both changed differently → conflict (defaults to taking left, flagged for resolution)

**Conflict resolution** iterates all conflicts in a MergeResult and overwrites the merged sequence's value at the conflict position according to the chosen strategy. After resolution, the conflict list is cleared.

## Known Limitations

- **LCS diff is O(m × n) in time and space.** For very long ternary sequences (millions of elements), this is slow and memory-hungry. No patience diff, histogram diff, or rolling-hash optimization.
- **Three-way merge is position-based, not content-aligned.** If left inserts elements at the beginning and right at the end, every subsequent position shifts and produces false conflicts. A proper merge would align sequences first (similar to how git aligns text lines).
- **No partial resolution.** `ConflictResolver::resolve` applies the same strategy to all conflicts. There's no per-conflict override.
- **Similarity metric treats all operations equally.** An Insert and a Change both count as one "non-equal" operation. In some contexts, a small value change (0 → 1) might be less significant than a structural change (insertion/deletion).

## Use Cases

- **Sensor stream comparison** — Diff two time windows of ternary sensor data to detect anomalies or drift.
- **Configuration versioning** — Track changes to ternary feature flags across deployments; merge concurrent edits from different teams.
- **Collaborative editing of ternary sequences** — Multiple agents modifying a shared ternary state (cell tissue, voting record) with conflict detection and resolution.

## Ecosystem Context

Part of the SuperInstance ternary crate family. `ternary-diff` is a utility layer used by any crate that needs to compare, merge, or version ternary data. It works on the raw `Trit` values produced by `ternary-cell`, `ternary-voting`, `ternary-sensor`, and `ternary-language-model`, and can diff the outputs of `ternary-visualization` chart data.

## License

MIT
