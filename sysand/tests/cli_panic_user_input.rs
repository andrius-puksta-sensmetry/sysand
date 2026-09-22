// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: © 2026 Sysand contributors <opensource@sensmetry.com>

//! Known panics reachable from user input.
//!
//! Every test here feeds Sysand nothing but ordinary user-supplied input --
//! a project file, `.sysand/env.toml`, a `sysand.toml` configuration file or
//! a command-line flag -- and the CLI aborts with a panic instead of
//! reporting an error. None of them involve I/O failures, allocation or a
//! missing file; the input is readable and parses, and a `panic!`,
//! `unreachable!`, `assert!` or `unwrap` further in is what gives way.
//!
//! The tests are marked `#[should_panic]` so that they pass while the bugs
//! are open and fail once one is fixed. When fixing a bug, replace the
//! attribute with an assertion on the exit code and the reported error.
//!
//! The CLI is driven in-process through [`sysand::lib_main`] rather than as
//! a subprocess, because `#[should_panic]` can only observe a panic that
//! unwinds through the test itself. Sysand locates the project from the
//! working directory, which is per-process, so the tests take [`CWD`] and
//! run one at a time.

use std::sync::{Mutex, MutexGuard, PoisonError};

use camino::{Utf8Path, Utf8PathBuf};
use camino_tempfile::Utf8TempDir;

/// Serializes the working directory across the tests in this binary.
/// Poisoning is expected and ignored: every test in this file panics by
/// design, and the guarded state is the process working directory, which
/// each test sets for itself before use.
static CWD: Mutex<()> = Mutex::new(());

fn lock_cwd() -> MutexGuard<'static, ()> {
    CWD.lock().unwrap_or_else(PoisonError::into_inner)
}

const INFO_DEMO: &str = r#"{
  "name": "demo",
  "publisher": "acme",
  "version": "0.0.1",
  "usage": [{ "resource": "pkg:sysand/acme/other", "versionConstraint": null }]
}"#;

const INFO_DEMO_USING_SUB: &str = r#"{
  "name": "demo",
  "publisher": "acme",
  "version": "0.0.1",
  "usage": [{ "dir": "sub", "publisher": "acme", "name": "other" }]
}"#;

const INFO_OTHER: &str = r#"{ "name": "other", "publisher": "acme", "version": "0.0.1" }"#;

const META: &str = r#"{ "index": {}, "created": "2026-01-01T00:00:00Z" }"#;

/// A scratch directory holding the files a test hands to Sysand. Kept alive
/// for the duration of the test so the directory outlives the CLI run.
struct Scratch {
    dir: Utf8TempDir,
}

impl Scratch {
    fn new() -> Self {
        Self {
            dir: camino_tempfile::tempdir().expect("failed to create a temporary directory"),
        }
    }

    fn path(&self) -> &Utf8Path {
        self.dir.path()
    }

    /// Write `contents` to `relative`, creating parent directories.
    fn write(&self, relative: impl AsRef<Utf8Path>, contents: &str) -> &Self {
        let path = self.dir.path().join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("failed to create a scratch directory");
        }
        std::fs::write(&path, contents).expect("failed to write a scratch file");
        self
    }

    /// A project that uses `pkg:sysand/acme/other`, with an `other` project
    /// in `sub/`.
    fn with_demo_project(&self, info: &str) -> &Self {
        self.write(".project.json", info)
            .write(".meta.json", META)
            .write("sub/.project.json", INFO_OTHER)
            .write("sub/.meta.json", META)
    }

    /// Run the CLI as if invoked from this directory, and return its exit
    /// code. `--no-config` keeps the developer's own `sysand.toml` out of
    /// the run; a test that needs a configuration file passes
    /// `--config-file` as well, which is read regardless.
    fn run_sysand(&self, args: &[&str]) -> u8 {
        std::env::set_current_dir(self.dir.path()).expect("failed to enter the scratch directory");
        let argv = ["sysand", "--no-config"]
            .into_iter()
            .chain(args.iter().copied());
        sysand::lib_main(argv)
    }
}

/// `.sysand/env.toml` records an editable project by a path relative to the
/// environment's parent directory, and Sysand itself writes `path = "."`
/// there for a project that is its own environment's root -- `sysand init`
/// followed by `sysand sync` produces exactly that entry. Resolving such an
/// entry walks the path's components and treats `CurDir` as impossible.
///
/// Panics at `core/src/env/local_directory/mod.rs`, in
/// `LocalDirectoryEnvironment::get_project_storage`:
/// `internal error: entered unreachable code`.
///
/// A leading `./` (`path = "./sub"`) and an absolute `path = "/sub"` reach
/// the same `unreachable!()` through `Prefix`/`RootDir`. Only non-editable
/// entries are checked for those shapes when `env.toml` is parsed.
#[test]
#[should_panic(expected = "internal error: entered unreachable code")]
fn env_toml_editable_current_dir_path() {
    let _cwd = lock_cwd();
    let scratch = Scratch::new();
    scratch.with_demo_project(INFO_DEMO).write(
        ".sysand/env.toml",
        r#"version = "0.1"

[[project]]
publisher = "acme"
name = "other"
version = "0.0.1"
path = "."
identifiers = ["pkg:sysand/acme/other"]
editable = true
"#,
    );

    scratch.run_sysand(&["lock", "--no-index"]);
}

/// Installing a project into `.sysand` overwrites the entry already
/// recorded for that identifier and version, and asserts that the entry is
/// not an editable one. An `env.toml` that marks the same identifier and
/// version editable -- which nothing rejects when the file is read -- makes
/// the assertion fail.
///
/// Panics at `core/src/env/local_directory/mod.rs`, in
/// `<LocalDirectoryEnvironment as WriteEnvironment>::put_project`:
/// `assertion failed: !existing.editable`.
#[test]
#[should_panic(expected = "assertion failed: !existing.editable")]
fn env_toml_editable_entry_reinstalled() {
    let _cwd = lock_cwd();
    let scratch = Scratch::new();
    scratch.with_demo_project(INFO_DEMO_USING_SUB).write(
        ".sysand/env.toml",
        r#"version = "0.1"

[[project]]
publisher = "acme"
name = "other"
version = "0.0.1"
path = "sub"
identifiers = ["pkg:sysand/acme/other"]
editable = true
"#,
    );

    scratch.run_sysand(&["sync", "--no-index"]);
}

/// Choosing a directory for a newly installed project compares the
/// candidate name against the last component of every installed project's
/// path. `env.toml` is only checked for `.`/`..` components in
/// *non-editable* entries, so an editable entry may name a path that starts
/// with the installed prefix `lib/` and still has no final component.
///
/// Panics at `core/src/env/local_directory/mod.rs`, in
/// `LocalDirectoryEnvironment::compute_project_path`:
/// `called `Option::unwrap()` on a `None` value`.
#[test]
#[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
fn env_toml_installed_path_without_file_name() {
    let _cwd = lock_cwd();
    let scratch = Scratch::new();
    scratch.with_demo_project(INFO_DEMO_USING_SUB).write(
        ".sysand/env.toml",
        r#"version = "0.1"

[[project]]
publisher = "acme"
name = "unrelated"
version = "9.9.9"
path = "lib/x/.."
identifiers = ["pkg:sysand/acme/unrelated"]
editable = true
"#,
    );

    scratch.run_sysand(&["sync", "--no-index"]);
}

/// A checksum in `.sysand/env.toml` is carried into the lockfile source
/// Sysand generates for that project, but `env.toml` is not checked for
/// well-formed digests when it is read. Canonicalizing the generated lock
/// re-validates it and unwraps the result, so a short or non-hex
/// `src_cksum` aborts `sysand lock` rather than being reported.
///
/// Panics at `core/src/lock.rs`, in `Lock::canonicalize`:
/// `called `Result::unwrap()` on an `Err` value: InvalidDigestFormat { .. }`.
///
/// The offending `unwrap` is inside a `debug_assert!`, so this is a panic in
/// debug builds only; in a release build the malformed digest is written to
/// `sysand-lock.toml` unchecked instead.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "InvalidDigestFormat")]
fn env_toml_malformed_checksum_reaches_lock() {
    let _cwd = lock_cwd();
    let scratch = Scratch::new();
    scratch
        .with_demo_project(INFO_DEMO)
        .write(".sysand/lib/x/.project.json", INFO_OTHER)
        .write(".sysand/lib/x/.meta.json", META)
        .write(
            ".sysand/env.toml",
            r#"version = "0.1"

[[project]]
publisher = "acme"
name = "other"
version = "0.0.1"
path = "lib/x"
identifiers = ["pkg:sysand/acme/other"]
src_cksum = "00"
"#,
        );

    scratch.run_sysand(&["lock", "--no-index"]);
}

/// A `sysand.toml` may override where a project comes from. An `editable`
/// override is expected to be a path relative to the workspace root, but
/// nothing rejects an absolute one when the configuration file is read, and
/// the project wrapper that records it asserts the path is relative.
///
/// Panics at `core/src/project/editable.rs`, in `EditableProject::new`:
/// `assertion failed: nominal_path.is_relative()`.
///
/// The assertion is a `debug_assert!`, so this is a panic in debug builds
/// only; a release build writes the absolute path into `sysand-lock.toml`,
/// which `Source::to_toml` separately expects not to happen.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "assertion failed: nominal_path.is_relative()")]
fn config_editable_override_with_absolute_path() {
    let _cwd = lock_cwd();
    let scratch = Scratch::new();
    scratch.with_demo_project(INFO_DEMO);

    let absolute: Utf8PathBuf = scratch.path().join("sub");
    scratch.write(
        "sysand.toml",
        &format!(
            r#"[[project]]
identifiers = ["pkg:sysand/acme/other"]
sources = [{{ editable = "{absolute}" }}]
"#
        ),
    );

    scratch.run_sysand(&["--config-file", "sysand.toml", "lock", "--no-index"]);
}
