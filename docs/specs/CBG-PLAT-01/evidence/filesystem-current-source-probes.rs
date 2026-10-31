include!("/tmp/codebasegraph-pr153-platform-source-862a8f88d485/src/storage/test_harness.rs");

#[test]
fn pr153_review_macos_nested_scan_does_not_consume_outer_cursor() {
    use storage::locks::{path_identity, RootedDirectory};
    let fixture = temp_dir("pr153-nested-scan");
    for index in 0..200 {
        fs::write(fixture.join(format!("entry-{index:04}")), b"one").unwrap();
    }
    let root = RootedDirectory::open_readonly_root(&fixture, path_identity(&fixture).unwrap()).unwrap();
    let mut expected = 0usize;
    root.visit_entries(&mut [0;4096], &mut |_| {expected += 1; Ok(())}).unwrap();
    let mut outer = 0usize;
    let mut inner_min = usize::MAX;
    root.visit_entries(&mut [0;4096], &mut |_| {
        let mut inner = 0usize;
        root.visit_entries(&mut [0;4096], &mut |_| {inner += 1; Ok(())})?;
        inner_min = inner_min.min(inner);
        outer += 1;
        Ok(())
    }).unwrap();
    eprintln!("baseline={expected}, outer={outer}, inner_min={inner_min}");
    drop(root);
    fs::remove_dir_all(&fixture).unwrap();
    assert_eq!(inner_min, expected);
    assert_eq!(outer, expected, "same-parent nested scan must preserve the outer cursor");
}
