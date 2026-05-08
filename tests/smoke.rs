use dev_fixtures::TempProject;

#[test]
fn build_simple_project() {
    let project = TempProject::new()
        .with_file("Cargo.toml", "[package]\nname = \"x\"\nversion = \"0.0.0\"\n")
        .with_file("src/lib.rs", "pub fn x() {}")
        .build()
        .unwrap();
    assert!(project.path().join("Cargo.toml").exists());
    assert!(project.path().join("src/lib.rs").exists());
}

#[test]
fn build_with_binary_file() {
    let project = TempProject::new()
        .with_bytes("data.bin", vec![0u8, 1, 2, 3, 255])
        .build()
        .unwrap();
    let bytes = std::fs::read(project.path().join("data.bin")).unwrap();
    assert_eq!(bytes, vec![0u8, 1, 2, 3, 255]);
}
