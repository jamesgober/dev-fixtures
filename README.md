<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <strong>dev-fixtures</strong>
    <br>
    <sup><sub>DETERMINISTIC TEST FIXTURES FOR RUST</sub></sup>
</h1>
<p align="center">
    <a href="https://crates.io/crates/dev-fixtures"><img alt="crates.io" src="https://img.shields.io/crates/v/dev-fixtures.svg"></a>
    <a href="https://crates.io/crates/dev-fixtures"><img alt="downloads" src="https://img.shields.io/crates/d/dev-fixtures.svg"></a>
    <a href="https://github.com/jamesgober/dev-fixtures/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/jamesgober/dev-fixtures/actions/workflows/ci.yml/badge.svg"></a>
    <img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.85%2B-blue.svg?style=flat-square" title="Rust Version">
    <a href="https://docs.rs/dev-fixtures"><img alt="docs.rs" src="https://docs.rs/dev-fixtures/badge.svg"></a>
</p>

<p align="center">
    <strong>Test environments and sample data that don't drift run-to-run.</strong> Build a disposable project tree, run your code against it, drop it on exit.
</p>

<br>

<div align="center">
    <strong>Part of the <a href="https://crates.io/crates/dev-tools"><code>dev-*</code></a> verification collection.</strong><br>
    <sub>Also available as the <code>fixtures</code> feature of the <a href="https://crates.io/crates/dev-tools"><code>dev-tools</code></a> umbrella crate &mdash; one dependency, every verification layer.</sub>
</div>

<br>

---

## What it does

Builds disposable, deterministic test environments. The most common
primitive is `TempProject`, which lets you stage a project tree, run
your test against it, and have the directory cleaned up on drop.

## Quick start

```toml
[dependencies]
dev-fixtures = "0.9.4"
```

```rust
use dev_fixtures::TempProject;

let project = TempProject::new()
    .with_file("Cargo.toml", "[package]\nname = \"sample\"\n")
    .with_file("src/lib.rs", "pub fn answer() -> u32 { 42 }")
    .build()
    .unwrap();

// project.path() points at a temp directory.
// It is deleted automatically when `project` is dropped.
```

## File-tree builders

```rust
use dev_fixtures::tree::{rust_workspace, FileTree};

let dir = tempfile::tempdir().unwrap();

// One-line workspace.
rust_workspace(dir.path(), &["alpha", "beta"]).unwrap();

// Or build trees by hand.
FileTree::new(dir.path())
    .file("README.md", "hello")
    .dir("data")
    .file("data/sample.txt", "...")
    .build()
    .unwrap();
```

## Adversarial inputs

```rust
use dev_fixtures::adversarial;

let dir = tempfile::tempdir().unwrap();

adversarial::oversized_zeros(&dir.path().join("big.bin"), 1_000_000).unwrap();
adversarial::malformed_utf8(&dir.path().join("bad.txt")).unwrap();
adversarial::random_bytes(&dir.path().join("rand.bin"), 1024, 42).unwrap();

let names = adversarial::unusual_names(5); // emoji, Unicode, dotted, etc.
```

## Golden snapshots

```rust
use dev_fixtures::golden::Golden;

let dir = tempfile::tempdir().unwrap();
let g = Golden::new(dir.path().join("snap.txt"));

// First call creates the snapshot (verdict=Skip, tag=created).
let _ = g.compare("greet", "hello\n");
// Subsequent calls verify (Pass / Fail with diff).
let check = g.compare("greet", "hello\n");
assert!(matches!(check.verdict, dev_report::Verdict::Pass));
```

Set `DEV_FIXTURES_UPDATE_GOLDEN=1` to regenerate snapshots on
intentional changes.

## Mock data

```rust
use dev_fixtures::mock::{bytes, csv, json_array, Rng};

// Deterministic CSV.
let csv = csv::generate(&["id", "name"], 10, 42, |rng| {
    vec![rng.range(1000).to_string(), format!("user_{}", rng.range(100))]
});

// Deterministic JSON array.
let json = json_array::generate(10, 42, |rng| format!("{{\"id\":{}}}", rng.range(1000)));

// Raw bytes.
let zeroed = bytes::zeros(1024);
let patterned = bytes::patterned(1024, &[0xDE, 0xAD, 0xBE, 0xEF]);
let random = bytes::random(1024, 42);
```

## dev-report integration

Wrap a fixture lifecycle as a `Producer`:

```rust
use dev_fixtures::{FixtureProducer, TempProject};
use dev_report::Producer;

let producer = FixtureProducer::new(
    "temp_project_lifecycle",
    "0.1.0",
    || {
        let _p = TempProject::new()
            .with_file("README.md", "hello")
            .build()?;
        Ok(())
    },
);
let _report = producer.produce();
```

Or use `Fixture::set_up_checked(name)` directly to emit a
`CheckResult` tagged `fixtures` (and `setup_failed` + `regression`
on failure).

## The `dev-*` collection

`dev-fixtures` ships independently and also slots into the
[`dev-tools`](https://crates.io/crates/dev-tools) umbrella crate as
the `fixtures` feature. The other crates in the collection cover the
rest of the verification dimensions:

- [`dev-report`](https://crates.io/crates/dev-report) &mdash; report schema everything emits
- [`dev-bench`](https://crates.io/crates/dev-bench) &mdash; performance and regression detection
- [`dev-async`](https://crates.io/crates/dev-async) &mdash; async runtime verification
- [`dev-stress`](https://crates.io/crates/dev-stress) &mdash; stress and soak workloads
- [`dev-chaos`](https://crates.io/crates/dev-chaos) &mdash; fault injection and recovery testing
- [`dev-coverage`](https://crates.io/crates/dev-coverage) &mdash; code coverage with regression gates
- [`dev-security`](https://crates.io/crates/dev-security) &mdash; CVE / license / banned-crate audit
- [`dev-deps`](https://crates.io/crates/dev-deps) &mdash; unused / outdated dep detection
- [`dev-ci`](https://crates.io/crates/dev-ci) &mdash; GitHub Actions workflow generator
- [`dev-fuzz`](https://crates.io/crates/dev-fuzz) &mdash; fuzz testing workflow
- [`dev-flaky`](https://crates.io/crates/dev-flaky) &mdash; flaky-test detection
- [`dev-mutate`](https://crates.io/crates/dev-mutate) &mdash; mutation testing

Pull each crate individually, or use
[`dev-tools`](https://crates.io/crates/dev-tools) with the appropriate
feature flag.

## Status

`v0.9.x` is the pre-1.0 stabilization line. APIs are expected to be
near-final; minor adjustments may still happen ahead of `1.0`.
Cleanup-on-drop and determinism guarantees are the contract.

## Cross-platform support

Linux, macOS, and Windows. Symlink helpers are Unix-only and
silently no-op on Windows; all other features behave identically
across platforms.

## Minimum supported Rust version

`1.85` — pinned in `Cargo.toml` via `rust-version` and verified by
the MSRV job in CI. (Bumped from 1.75 because `tempfile`'s
transitive dep `getrandom` requires `edition2024`, stabilized in
Rust 1.85.)

## License

Apache-2.0. See [LICENSE](LICENSE).




<!-- COPYRIGHT
---------------------------------->
<div align="center">
    <br>
    <h2></h2>
    Copyright &copy; 2026 James Gober.
</div>
