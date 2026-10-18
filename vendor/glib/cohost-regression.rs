//! App-maintained regression for the upstream VariantStrIter soundness fix.
//! Run in release mode on Linux: the original UB manifests under optimization.
#![cfg(target_os = "linux")]

use glib::prelude::*;

#[test]
fn borrowed_string_iterator_survives_optimized_out_pointer_writes() {
    let values = ["Nova", "Grüße", "🦊", "", "last"];
    let variant = values.as_slice().to_variant();

    // Each call exercises VariantStrIter::impl_get on a non-empty string array.
    let mut forward = variant.array_iter_str().unwrap();
    assert_eq!(forward.next(), Some("Nova"));
    assert_eq!(forward.nth(1), Some("🦊"));
    assert_eq!(forward.last(), Some("last"));

    let mut backward = variant.array_iter_str().unwrap();
    assert_eq!(backward.next_back(), Some("last"));
    assert_eq!(backward.nth_back(1), Some("🦊"));
    assert_eq!(backward.next_back(), Some("Grüße"));

    assert_eq!(variant.array_iter_str().unwrap().collect::<Vec<_>>(), values);
    assert_eq!(variant.array_iter_str().unwrap().rev().collect::<Vec<_>>(), values.into_iter().rev().collect::<Vec<_>>());

    let empty: &[&str] = &[];
    assert_eq!(empty.to_variant().array_iter_str().unwrap().next(), None);
}
