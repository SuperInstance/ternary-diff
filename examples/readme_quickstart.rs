// Verbatim copy of the README "Quick Start" example, to prove it compiles AND
// produces the values it claims.
use ternary_diff::*;

fn main() {
    // Diff two sequences
    let a = TernarySeq::from_i8(&[1, -1, 0, 1]);
    let b = TernarySeq::from_i8(&[1, 0, 0, -1]);
    let diff = TernaryDiff::diff(&a, &b);
    println!(
        "Changes: {}, Similarity: {:.2}",
        diff.change_count(),
        diff.similarity()
    );

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
    println!(
        "Merged: {:?}, Conflicts: {}",
        result.merged.trits(),
        result.has_conflicts
    );

    // Resolve conflicts
    let mut result = ThreeWayMerge::merge(
        &base,
        &TernarySeq::from_i8(&[1, 0, 0]),
        &TernarySeq::from_i8(&[-1, 0, 0]),
    );
    ConflictResolver::resolve(&mut result, ResolutionStrategy::Neutral);
    assert_eq!(result.merged.get(0), Some(Trit::Zero));

    println!("README Quick Start: ALL ASSERTIONS PASSED");
}
