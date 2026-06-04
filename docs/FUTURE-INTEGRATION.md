# Future Integration: ternary-diff

## Current State
Provides diff, patch, and merge for ternary sequences: `TernaryDiff` computes changes between `TernarySeq` instances, `TernaryPatch` applies patches, `ThreeWayMerge` performs three-way merges with conflict detection, and `ConflictResolver` provides strategies for resolving merge conflicts.

## Integration Opportunities

### With ternary-cell (State Change Tracking)
Every ternary-cell tick modifies cell state. ternary-diff tracks these modifications as patches — a `TernaryDiff` between tick N and tick N-1 captures exactly what changed. These diffs compress well (cells change slowly) and can be transmitted via ternary-protocol instead of full state. The receiver applies `TernaryPatch` to reconstruct the current state from a base state + incremental diffs.

### With ternary-replay (Event History Compression)
ternary-replay records experiment histories step by step. ternary-diff compresses these histories: instead of storing every step's full state, store the base state and a sequence of diffs. `ThreeWayMerge` enables branching replays — two variant experiments from the same seed can be compared and merged, with conflict detection showing where the experiments diverged.

### With ternary-lattice (Structured Conflict Resolution)
ternary-lattice's `TernaryLattice::join()` provides a deterministic conflict resolution strategy for ternary-diff. When a `ThreeWayMerge` finds conflicting changes, the lattice join selects the most informative value (concrete ±1 over unknown 0). This is a domain-specific `ConflictResolver` that respects the ternary information ordering.

## Potential in Mature Systems
In room-as-codespace, PLATO synchronizes tile stores between rooms. Instead of sending entire tile stores, rooms exchange ternary-diffs — compact summaries of what changed. When two rooms modify the same tile concurrently, `ThreeWayMerge` detects the conflict and `ConflictResolver` resolves it using domain-specific rules (e.g., the room with higher surprise wins). This is git-style distributed synchronization for room state.

## Cross-Pollination Ideas
- **ternary-steganography**: Hide data in diffs — the `ConflictResolver::Custom` variant can encode hidden information in its resolution choices, creating a covert channel in the diff stream.
- **ternary-codes**: Error-correcting diffs — encode diffs with ternary-codes' Hamming protection before transmission, so corrupted patches can be corrected.
- **ternary-causality**: Causal diffs — annotate diff hunks with causal labels so the consumer knows which changes are causes vs. effects.

## Dependencies for Next Steps
- Define `TileDiff` as `TernaryDiff` over tile store sequences
- Add diff serialization to ternary-protocol wire format
- Implement lattice-based `ConflictResolver` using ternary-lattice
- Benchmark diff computation on typical room state sizes (100-10000 tiles)
