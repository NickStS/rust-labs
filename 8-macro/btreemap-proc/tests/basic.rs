use std::collections::BTreeMap;

use btreemap_proc::btreemap;

#[test]
fn empty() {
    let m: BTreeMap<i32, i32> = btreemap!();
    assert!(m.is_empty());
}

#[test]
fn basic_pairs() {
    let m = btreemap!(
        3 => "c",
        1 => "a",
        2 => "b",
    );

    let mut expected = BTreeMap::new();
    expected.insert(1, "a");
    expected.insert(2, "b");
    expected.insert(3, "c");

    assert_eq!(m, expected);
}

#[test]
fn overwrite_key() {
    let m = btreemap!(
        "k" => 1,
        "k" => 2
    );

    let mut expected = BTreeMap::new();
    expected.insert("k", 2);

    assert_eq!(m, expected);
}
