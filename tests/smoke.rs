use dev_fixtures::{
    adversarial,
    golden::Golden,
    mock::{bytes as mock_bytes, csv as mock_csv, json_array as mock_json, Rng},
    tree::{rust_crate, rust_workspace, FileTree},
    Fixture, FixtureProducer, TempProject,
};
use dev_report::Producer;

#[test]
fn build_simple_project() {
    let project = TempProject::new()
        .with_file(
            "Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.0.0\"\n",
        )
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

struct OkFixture;
impl Fixture for OkFixture {
    type Output = ();
    fn set_up(&mut self) -> std::io::Result<()> {
        Ok(())
    }
    fn tear_down(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn smoke_set_up_checked_carries_tags() {
    let c = OkFixture.set_up_checked("ok");
    assert!(c.has_tag("fixtures"));
    assert!(matches!(c.verdict, dev_report::Verdict::Pass));
}

#[test]
fn smoke_fixture_producer_emits_report() {
    let producer = FixtureProducer::new("temp_lifecycle", "0.1.0", || {
        let _p = TempProject::new().with_file("README.md", "hello").build()?;
        Ok(())
    });
    let report = producer.produce();
    assert_eq!(report.checks.len(), 1);
    assert_eq!(report.producer.as_deref(), Some("dev-fixtures"));
    assert!(matches!(
        report.overall_verdict(),
        dev_report::Verdict::Pass
    ));
}

#[test]
fn smoke_file_tree_workspace_layout() {
    let dir = mod_tempdir::TempDir::new().unwrap();
    rust_workspace(dir.path(), &["a", "b"]).unwrap();
    assert!(dir.path().join("a/Cargo.toml").exists());
    assert!(dir.path().join("b/src/lib.rs").exists());
}

#[test]
fn smoke_file_tree_basic() {
    let dir = mod_tempdir::TempDir::new().unwrap();
    FileTree::new(dir.path())
        .file("a.txt", "hello")
        .dir("d")
        .build()
        .unwrap();
    assert!(dir.path().join("a.txt").exists());
    assert!(dir.path().join("d").is_dir());
}

#[test]
fn smoke_rust_crate_helper() {
    let dir = mod_tempdir::TempDir::new().unwrap();
    rust_crate(dir.path(), "alpha", "0.1.0").unwrap();
    let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
    assert!(cargo.contains("name = \"alpha\""));
}

#[test]
fn smoke_adversarial_oversized_and_random() {
    let dir = mod_tempdir::TempDir::new().unwrap();
    let path = dir.path().join("big.bin");
    adversarial::oversized_zeros(&path, 4096).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 4096);

    let p2 = dir.path().join("rand.bin");
    adversarial::random_bytes(&p2, 64, 42).unwrap();
    assert_eq!(std::fs::read(&p2).unwrap().len(), 64);
}

#[test]
fn smoke_adversarial_malformed_utf8_round_trip() {
    let dir = mod_tempdir::TempDir::new().unwrap();
    let path = dir.path().join("bad.txt");
    adversarial::malformed_utf8(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(std::str::from_utf8(&bytes).is_err());
}

#[test]
fn smoke_golden_first_run_creates_then_matches() {
    let dir = mod_tempdir::TempDir::new().unwrap();
    let path = dir.path().join("snap.txt");
    let g = Golden::new(&path);
    let first = g.compare("greet", "hello\n");
    assert!(matches!(first.verdict, dev_report::Verdict::Skip));
    let second = g.compare("greet", "hello\n");
    assert!(matches!(second.verdict, dev_report::Verdict::Pass));
}

#[test]
fn smoke_golden_mismatch_yields_diff() {
    let dir = mod_tempdir::TempDir::new().unwrap();
    let path = dir.path().join("snap.txt");
    std::fs::write(&path, "expected\n").unwrap();
    let c = Golden::new(&path).compare("x", "actual\n");
    assert!(matches!(c.verdict, dev_report::Verdict::Fail));
    assert!(c.has_tag("regression"));
    assert!(c.detail.as_deref().unwrap().contains("-expected"));
    assert!(c.detail.as_deref().unwrap().contains("+actual"));
}

#[test]
fn smoke_mock_csv_round_trip() {
    let csv1 = mock_csv::generate(&["id", "name"], 3, 7, |rng| {
        vec![rng.range(100).to_string(), format!("u{}", rng.range(10))]
    });
    let csv2 = mock_csv::generate(&["id", "name"], 3, 7, |rng| {
        vec![rng.range(100).to_string(), format!("u{}", rng.range(10))]
    });
    assert_eq!(csv1, csv2);
    assert!(csv1.starts_with("id,name\n"));
}

#[test]
fn smoke_mock_json_array_shape() {
    let json = mock_json::generate(2, 0, |rng| format!("{{\"v\":{}}}", rng.range(10)));
    assert!(json.starts_with("[") && json.ends_with("]"));
}

#[test]
fn smoke_mock_bytes_random_deterministic() {
    let a = mock_bytes::random(32, 7);
    let b = mock_bytes::random(32, 7);
    assert_eq!(a, b);
}

#[test]
fn smoke_rng_deterministic() {
    let mut a = Rng::seeded(42);
    let mut b = Rng::seeded(42);
    assert_eq!(a.next_u64(), b.next_u64());
}
