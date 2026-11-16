// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: © 2026 Sysand contributors <opensource@sensmetry.com>

use std::assert_matches;

use typed_path::{Utf8UnixPath, Utf8UnixPathBuf};

use crate::manifest::common::{
    ExpandError, ExpandProblem, InheritFrom, InheritMarker, Maintainer, PathError, PathKey,
    PathProblem, SourceDirEntry, SourceDirEntryKind, Sources, SourcesError, StdlibStem,
    admitted_source, is_valid_group_name, normalized_identity, path_problem, root_path,
    segment_matches, wildcard_matches,
};
use crate::model::{IndexName, IndexPublisher, KnownMetamodel};

fn sources(entries: &[&str]) -> Result<Sources, SourcesError> {
    Sources::new(
        entries
            .iter()
            .map(|entry| Utf8UnixPathBuf::from(*entry))
            .collect(),
    )
}

fn sources_error(entries: &[&str]) -> SourcesError {
    sources(entries).expect_err("entries should be rejected")
}

/// The accepted syntax is given as regexes; publishers do not admit `.`
/// as a separator, names do.
#[test]
fn identifier_syntax_follows_the_documented_regexes() {
    for ok in ["acme", "Acme Corp", "a-b", "ab1"] {
        assert!(
            IndexPublisher::parse(ok).is_ok(),
            "{ok} should be a publisher"
        );
    }
    for bad in ["ab", "-acme", "acme-", "acme--corp", "acme.corp", "açme"] {
        assert!(
            IndexPublisher::parse(bad).is_err(),
            "{bad} should not be a publisher"
        );
    }
    IndexName::parse("foo.bar").unwrap();
    IndexName::parse("foo..bar").unwrap_err();
    IndexName::parse("a".repeat(51)).unwrap_err();

    let name = IndexName::parse("Widget Kit").expect("a valid name");
    assert_eq!(name.as_str(), "Widget Kit");
    assert_eq!(name.normalized(), "widget-kit");
}

#[test]
fn identifier_error_names_the_rule() {
    let publisher = IndexPublisher::parse("ab").expect_err("too short");
    assert_eq!(
        publisher.to_string(),
        "publisher must be 3-50 ASCII alphanumeric chars, with single ` ` or `-` separators between words"
    );

    let name = IndexName::parse("ab").expect_err("too short");
    assert_eq!(
        name.to_string(),
        "name must be 3-50 ASCII alphanumeric chars, with single ` `, `-`, or `.` separators between words"
    );
}

/// Projects are resolved by their normalized identity, which is also what
/// the lockfile records.
#[test]
fn normalized_identity_is_a_pkg_sysand_purl() {
    let publisher = IndexPublisher::parse("Acme Corp").expect("a valid publisher");
    let name = IndexName::parse("Widget Kit").expect("a valid name");
    let identity = normalized_identity(&publisher, &name);

    assert_eq!(identity.to_string(), "acme-corp/widget-kit");
    assert_eq!(
        identity.lock_identifier(),
        "pkg:sysand/acme-corp/widget-kit"
    );
}

/// A publisher is the standard-library publisher iff it normalizes to
/// `omg`.
#[test]
fn stdlib_publisher_is_recognized_after_normalization() {
    for publisher in ["omg", "OMG", "Omg"] {
        assert!(
            IndexPublisher::parse(publisher)
                .expect("a valid publisher")
                .is_stdlib_publisher(),
            "{publisher} should be the standard-library publisher"
        );
    }
    assert!(
        !IndexPublisher::parse("omgx")
            .expect("a valid publisher")
            .is_stdlib_publisher()
    );
}

/// Each maintainer entry is a string of the form `name <email>`, both
/// parts mandatory.
#[test]
fn maintainer_string_must_match_name_email_format() {
    let error = Maintainer::parse("no email here").expect_err("not a maintainer");
    assert!(
        error.to_string().contains("name <email>"),
        "unexpected error: {error}"
    );

    for bad in [
        "<a@b.c>",
        "A B <>",
        "A B",
        "A B <a@b.c> trailing",
        "A <x> trailing <y>",
        "A <<b>>",
        "A < b >",
        "A <b c>",
        "A> <b>",
        "A<b>",
        " <b>",
        "A   <b>",
        " A <b>",
        "A\t <b>",
        "A <b>",
    ] {
        assert!(Maintainer::parse(bad).is_err(), "{bad} is not a maintainer");
    }

    let maintainer = Maintainer::parse("A B <a@b.c>").expect("a maintainer");
    assert_eq!(maintainer.name, "A B");
    assert_eq!(maintainer.email, "a@b.c");
    assert_eq!(maintainer.to_string(), "A B <a@b.c>");
}

/// A maintainer displays as the text it was parsed from, so the
/// `.project.json` `maintainer` array keeps its spelling.
#[test]
fn maintainer_keeps_its_spelling() {
    for raw in ["A <b@c>", "A B <a@b.c>"] {
        let maintainer = Maintainer::parse(raw).expect("a maintainer");
        assert_eq!(maintainer.to_string(), raw);
    }
    let maintainer = Maintainer::parse("A <b@c>").expect("a maintainer");
    assert_eq!(maintainer.name, "A");
    assert_eq!(maintainer.email, "b@c");
}

/// The `stdlib` value is the last segment of the IRI at which the
/// library's KPAR is published, validated as a single path segment.
#[test]
fn stdlib_key_is_a_path_segment() {
    for ok in ["Kernel-Libraries", "Systems-Library", "a1"] {
        assert!(StdlibStem::parse(ok).is_ok(), "{ok} should be a stem");
    }
    for bad in ["", "-x", "a/b", "..", "a b", "a.kpar"] {
        assert!(
            StdlibStem::parse(bad).is_err(),
            "{bad} should not be a stem"
        );
    }
    assert_eq!(
        StdlibStem::parse("a/b")
            .expect_err("not a stem")
            .to_string(),
        "`stdlib` value `a/b` is not a single IRI path segment (`[A-Za-z0-9][A-Za-z0-9-]*`)"
    );
}

/// How a message names the group a marker selects.
#[test]
fn inherit_from_names_its_group() {
    assert_eq!(
        InheritFrom::Group("legacy".to_owned()).to_string(),
        "group `legacy`"
    );
    assert_eq!(InheritFrom::DefaultGroup.to_string(), "the default group");
}

/// There is no "inherit nothing" marker; a key that is not inherited
/// carries its own value.
#[test]
fn workspace_false_is_not_a_marker() {
    let error = InheritFrom::try_from(InheritMarker::Flag(false)).expect_err("not a marker");
    assert_eq!(
        error.to_string(),
        "`workspace = false` is not allowed; a key that is not inherited carries its own value"
    );

    let invalid =
        InheritFrom::try_from(InheritMarker::Group("bad name".to_owned())).expect_err("not a name");
    assert_eq!(
        invalid.to_string(),
        "`bad name` is not a valid workspace group name (a bare TOML key: letters, digits, \
         `-` and `_`)"
    );
}

/// A `<group-name>` is a bare TOML key: letters, digits, `-` and `_`.
#[test]
fn group_names_are_bare_toml_keys() {
    for ok in ["a_b-1", "legacy", "A1"] {
        assert!(is_valid_group_name(ok), "{ok} should be a group name");
    }
    for bad in ["a b", "a.b", "", "a/b"] {
        assert!(
            !is_valid_group_name(bad),
            "{bad} should not be a group name"
        );
    }
}

/// Entries are taken exactly as written: the manifest admits only the one
/// form Sysand itself writes.
#[test]
fn sources_entries_are_taken_exactly_as_written() {
    for (entries, expected) in [
        (&["/abs/main.sysml"][..], "is absolute"),
        // Nothing is resolved, so a `..` is a form error and no entry can
        // leave the project root.
        (
            &["../outside.sysml"],
            "is not in the form a `sources` entry takes",
        ),
        (
            &["src/../../outside.sysml"],
            "is not in the form a `sources` entry takes",
        ),
        (
            &["./main.sysml"],
            "is not in the form a `sources` entry takes",
        ),
        (
            &["src/./main.sysml"],
            "is not in the form a `sources` entry takes",
        ),
        (
            &["src//main.sysml"],
            "is not in the form a `sources` entry takes",
        ),
        (&["."], "is not in the form a `sources` entry takes"),
        (&[".."], "is not in the form a `sources` entry takes"),
        (&[""], "is not in the form a `sources` entry takes"),
        (&["src/"], "names a directory"),
        (
            &["src/main.sysml", "src/main.sysml"],
            "is listed more than once",
        ),
    ] {
        let error = sources_error(entries);
        assert!(
            error.to_string().contains(expected),
            "{entries:?}: unexpected error: {error}"
        );
    }

    let stored =
        sources(&["main.sysml", "src/parts.kerml", "lib/util.kerml"]).expect("the strict form");
    assert_eq!(
        stored.as_slice(),
        [
            Utf8UnixPathBuf::from("main.sysml"),
            Utf8UnixPathBuf::from("src/parts.kerml"),
            Utf8UnixPathBuf::from("lib/util.kerml"),
        ]
    );

    // Equality is exact, so two spellings of one file on a
    // case-insensitive filesystem are two entries.
    sources(&["a.sysml", "A.sysml"]).expect("equality is exact");
}

/// A manifest is read the same way on every platform, so a Windows path
/// is not a source path. The drive letter makes `C:/main.sysml` absolute.
#[test]
fn sources_rejects_windows_paths() {
    assert!(
        sources_error(&["C:/main.sysml"])
            .to_string()
            .contains("is absolute")
    );
    assert!(
        sources_error(&["src\\main.sysml"])
            .to_string()
            .contains("backslash")
    );
}

/// Every entry names a KerML (`.kerml`) or SysML (`.sysml`) file, the
/// extension compared case-insensitively.
#[test]
fn sources_entries_must_be_model_files() {
    for ok in &["a.sysml", "A.SYSML", "b.KerML", "src/c.kerml"] {
        assert!(sources(&[ok]).is_ok(), "{ok} should be a source file");
    }
    for bad in &["readme.md", "LICENSE", "a.sysml.bak", "sources"] {
        assert!(
            sources(&[bad]).is_err(),
            "{bad} should not be a source file"
        );
    }
    assert_eq!(
        sources_error(&["readme.md"]).to_string(),
        "`sources` entry `readme.md` is not a KerML (`.kerml`) or SysML (`.sysml`) file"
    );
}

/// The half of the language rule the metamodel decides, which
/// `Sources` cannot answer because the value may be inherited.
#[test]
fn a_source_the_metamodel_does_not_admit_is_refused() {
    let kerml =
        KnownMetamodel::recognize("https://www.omg.org/spec/KerML/20250201").expect("a kind");
    let sysml =
        KnownMetamodel::recognize("https://www.omg.org/spec/SysML/20250201").expect("a kind");

    let entry = Utf8UnixPath::new("a.sysml");
    let error = admitted_source(PathKey::Sources, entry, &kerml)
        .expect_err("a SysML file in a KerML project");
    assert_eq!(
        error.message,
        "`sources` entry `a.sysml` is a SysML file, which the metamodel \
         `https://www.omg.org/spec/KerML/20250201` does not admit;\n\
         a KerML project lists `.kerml` files, a SysML project `.sysml` and `.kerml` files"
    );
    let error =
        admitted_source(PathKey::Root, entry, &kerml).expect_err("a SysML root in a KerML project");
    assert!(
        error
            .message
            .starts_with("`root` `a.sysml` is a SysML file"),
        "{}",
        error.message
    );
    admitted_source(PathKey::Sources, entry, &sysml).expect("a SysML file in a SysML project");
    admitted_source(PathKey::Sources, Utf8UnixPath::new("a.kerml"), &kerml)
        .expect("KerML in KerML");
    admitted_source(PathKey::Sources, Utf8UnixPath::new("a.kerml"), &sysml)
        .expect("KerML in SysML");

    // Not a model file at all: `PathProblem::NotModelFile`'s, and no
    // metamodel to name.
    admitted_source(PathKey::Sources, Utf8UnixPath::new("notes.txt"), &kerml)
        .expect("no language to judge");
}

/// The two `sources` form errors, each on its whole text. A trailing
/// slash is not a malformed spelling but a directory, and it gets the
/// message that says so.
#[test]
fn the_two_sources_form_errors_read_in_full() {
    let malformed = sources_error(&["a//b.sysml"]);
    assert_eq!(
        malformed.to_string(),
        "`sources` entry `a//b.sysml` is not in the form a `sources` entry takes: relative, \
         `/`-separated,\nwith no `.` or `..` component and no repeated or leading slash"
    );

    let directory = sources_error(&["src/"]);
    assert_eq!(
        directory.to_string(),
        "`sources` entry `src/` names a directory; it must name a model file (directories are not admitted)"
    );
}

/// When a path breaks several rules, the first of: absolute, a NUL or
/// backslash, a trailing slash, the component rules.
#[test]
fn the_first_broken_rule_is_reported() {
    for (path, problem) in [
        ("/src/", PathProblem::Absolute),
        ("src\\/", PathProblem::Backslash),
        ("../src/", PathProblem::Directory),
        ("", PathProblem::NotStrict),
        ("a//b/../c.txt", PathProblem::NotStrict),
    ] {
        assert_eq!(path_problem(path), Some(problem), "{path}");
    }
}

/// `root` is held to a `sources` entry's syntax, and there is one rule
/// set. Every problem the shared check finds reads the same for both
/// keys, apart from the key it names.
#[test]
fn sources_and_root_share_their_checks() {
    for (path, problem, sources_text) in [
        (
            "/abs.sysml",
            PathProblem::Absolute,
            "`sources` entry `/abs.sysml` is absolute; paths are relative to the project root",
        ),
        (
            "a\\b.sysml",
            PathProblem::Backslash,
            "`sources` entry `a\\b.sysml` contains a backslash `\\`;\npaths are `/`-separated, so \
             that a manifest reads the same way\non every operating system",
        ),
        (
            "a\0.sysml",
            PathProblem::Null,
            "`sources` entry `a\0.sysml` contains a NULL character",
        ),
        (
            "../up.sysml",
            PathProblem::NotStrict,
            "`sources` entry `../up.sysml` is not in the form a `sources` entry takes: \
             relative, `/`-separated,\nwith no `.` or `..` component and no repeated or \
             leading slash",
        ),
        (
            "src/",
            PathProblem::Directory,
            "`sources` entry `src/` names a directory; it must name a model file (directories are not admitted)",
        ),
        (
            "notes.txt",
            PathProblem::NotModelFile,
            "`sources` entry `notes.txt` is not a KerML (`.kerml`) or SysML (`.sysml`) file",
        ),
    ] {
        assert_eq!(path_problem(path), Some(problem), "{path}");
        assert_eq!(sources_error(&[path]).to_string(), sources_text, "{path}");
        let root = root_path(path).expect_err(path);
        assert_eq!(root, PathError::new(PathKey::Root, problem, path), "{path}");
        assert_eq!(
            root.to_string(),
            sources_text.replacen("`sources` entry", "`root`", 1),
            "{path}"
        );
    }
    assert_eq!(path_problem("src/a.sysml"), None);
    assert_eq!(
        root_path("src/a.sysml").map(|p| p.as_str().to_owned()),
        Ok("src/a.sysml".to_owned())
    );
}

/// A `*` matches any run of characters within one segment, the empty one
/// included; every other character matches itself.
#[test]
fn a_wildcard_segment_matches_any_run_of_characters() {
    for (pattern, name) in [
        ("*.kerml", "Base.kerml"),
        ("*.kerml", ".kerml"),
        ("B*.kerml", "Base.kerml"),
        ("*a*e*.kerml", "Base.kerml"),
        ("Base*.kerml", "Base.kerml"),
        ("*", ""),
        ("a*a", "aa"),
        ("?[a].kerml", "?[a].kerml"),
    ] {
        assert!(
            segment_matches(pattern, name),
            "`{pattern}` must match `{name}`"
        );
    }
    for (pattern, name) in [
        ("*.kerml", "Base.sysml"),
        ("*.kerml", "Base.KERML"),
        ("B*.kerml", "base.kerml"),
        ("a*a", "a"),
        ("*b*b", "ab"),
        ("?.kerml", "a.kerml"),
    ] {
        assert!(
            !segment_matches(pattern, name),
            "`{pattern}` must not match `{name}`"
        );
    }
}

/// A `*` never crosses a `/`.
#[test]
fn a_wildcard_matches_segment_by_segment() {
    let matches = |pattern: &str, path: &str| {
        wildcard_matches(Utf8UnixPath::new(pattern), Utf8UnixPath::new(path))
    };
    assert!(matches("*.kerml", "Base.kerml"));
    assert!(matches("*/Base.kerml", "lib/Base.kerml"));
    assert!(matches("lib/*.kerml", "lib/Base.kerml"));
    assert!(!matches("*.kerml", "lib/Base.kerml"));
    assert!(!matches("lib/*.kerml", "lib/sub/Base.kerml"));
    assert!(!matches("*/*.kerml", "Base.kerml"));
}

/// A wildcard entry passes the same checks as any other entry; only `**`
/// is refused.
#[test]
fn wildcard_entries_are_checked_as_written() {
    let entries = sources(&["*.kerml", "lib/*/B*.sysml", "Base.kerml"])
        .expect("wildcard entries are entries");
    assert!(entries.has_wildcards());
    assert!(!sources(&["Base.kerml"]).expect("literal").has_wildcards());

    for (entry, problem) in [
        ("**.kerml", PathProblem::RecursiveWildcard),
        ("**/Base.kerml", PathProblem::RecursiveWildcard),
        ("*", PathProblem::NotModelFile),
        ("Base.*", PathProblem::NotModelFile),
        ("Base.ker*", PathProblem::NotModelFile),
        ("lib/*/", PathProblem::Directory),
        ("../*.kerml", PathProblem::NotStrict),
    ] {
        assert_eq!(
            sources_error(&[entry]),
            SourcesError::path(problem, entry),
            "{entry}"
        );
    }
    assert_eq!(
        sources_error(&["**/a.kerml"]).to_string(),
        "`sources` entry `**/a.kerml` contains `**`, which is not supported;\n\
         a `*` matches any characters within one path segment"
    );
    assert_eq!(
        sources_error(&["*.kerml", "*.kerml"]),
        SourcesError::Duplicate("*.kerml".to_owned())
    );
}

/// `root` names a single file, so it cannot be a wildcard entry.
#[test]
fn root_is_never_a_wildcard() {
    let error = root_path("*.sysml").expect_err("a wildcard root");
    assert_eq!(
        error,
        PathError::new(PathKey::Root, PathProblem::Wildcard, "*.sysml")
    );
    assert_eq!(
        error.to_string(),
        "`root` `*.sysml` contains `*`; it must name a single file"
    );
}

/// A file tree for [`Sources::expand`]: every listed path is a file, and
/// the directories are the ones those paths imply.
fn lister(
    files: &'static [&'static str],
) -> impl FnMut(&Utf8UnixPath) -> Result<Option<Vec<SourceDirEntry>>, !> {
    move |dir| {
        let mut listed: Vec<SourceDirEntry> = Vec::new();
        for file in files {
            let Ok(rest) = Utf8UnixPath::new(file).strip_prefix(dir) else {
                continue;
            };
            let (name, kind) = match rest.as_str().split_once('/') {
                Some((name, _)) => (name, SourceDirEntryKind::Directory),
                None => (rest.as_str(), SourceDirEntryKind::File),
            };
            if !listed.iter().any(|entry| entry.name == name) {
                listed.push(SourceDirEntry {
                    name: name.to_owned(),
                    kind,
                });
            }
        }
        // Listing order is not sorted order.
        listed.reverse();
        Ok(Some(listed))
    }
}

fn expand(entries: &[&str], files: &'static [&'static str]) -> Result<Vec<String>, ExpandError<!>> {
    Ok(sources(entries)
        .expect("valid entries")
        .expand(lister(files))?
        .into_iter()
        .map(|path| path.to_string())
        .collect())
}

fn expand_problem(entries: &[&str], files: &'static [&'static str]) -> ExpandProblem {
    match expand(entries, files).expect_err("expansion should fail") {
        ExpandError::Problem(problem) => problem,
        ExpandError::List(never) => match never {},
    }
}

/// Entries keep their order; each wildcard entry becomes the files it
/// matches, sorted by path, and a literal entry is passed through.
#[test]
fn expansion_keeps_entry_order_and_sorts_matches() {
    const FILES: &[&str] = &[
        "b.kerml",
        "a.kerml",
        "notes.txt",
        "a.sysml",
        "lib/z.kerml",
        "lib/y.kerml",
        "lib/sub/x.kerml",
        "other/y.kerml",
        "dir.kerml/inner.kerml",
    ];
    assert_eq!(
        expand(&["lib/z.kerml", "*.kerml", "missing.kerml"], FILES).expect("expands"),
        ["lib/z.kerml", "a.kerml", "b.kerml", "missing.kerml"]
    );
    assert_eq!(
        expand(&["*/y.kerml"], FILES).expect("expands"),
        ["lib/y.kerml", "other/y.kerml"]
    );
    assert_eq!(
        expand(&["lib/*/*.kerml", "*.sysml"], FILES).expect("expands"),
        ["lib/sub/x.kerml", "a.sysml"]
    );
}

/// Literal entries are not listed, so a project whose files cannot be
/// listed expands as long as it has no wildcard entry.
#[test]
fn expansion_lists_only_for_wildcard_entries() {
    let entries = sources(&["a.kerml", "lib/b.kerml"]).expect("valid");
    let expanded = entries
        .expand(|_| -> Result<_, !> { Ok(None) })
        .expect("nothing to list");
    assert_eq!(expanded, entries.as_slice());

    let wildcard = sources(&["*.kerml"]).expect("valid");
    let error = wildcard
        .expand(|_| -> Result<_, !> { Ok(None) })
        .expect_err("cannot be listed");
    assert_matches!(
        error,
        ExpandError::Problem(ExpandProblem::Unlistable(entry)) if entry == "*.kerml"
    );
}

#[test]
fn a_wildcard_entry_that_matches_nothing_is_an_error() {
    const FILES: &[&str] = &["a.sysml", "lib/b.kerml"];
    let problem = expand_problem(&["a.sysml", "*.kerml"], FILES);
    assert_eq!(problem, ExpandProblem::NoMatch("*.kerml".to_owned()));
    assert_eq!(
        problem.to_string(),
        "`sources` entry `*.kerml` matches no file"
    );
    assert_eq!(
        expand_problem(&["missing/*.kerml"], FILES),
        ExpandProblem::NoMatch("missing/*.kerml".to_owned())
    );
}

/// A file named by a wildcard entry and by any other entry is an error,
/// whichever comes first.
#[test]
fn a_file_named_by_two_entries_is_an_error() {
    const FILES: &[&str] = &["a.kerml", "ab.kerml"];
    let problem = expand_problem(&["a.kerml", "a*.kerml"], FILES);
    assert_eq!(
        problem,
        ExpandProblem::Overlap {
            path: "a.kerml".to_owned(),
            first: "a.kerml".to_owned(),
            second: "a*.kerml".to_owned(),
        }
    );
    assert_eq!(
        problem.to_string(),
        "`a.kerml` is named by both `sources` entry `a.kerml` and `a*.kerml`"
    );
    assert_eq!(
        expand_problem(&["*b.kerml", "a*.kerml"], FILES),
        ExpandProblem::Overlap {
            path: "ab.kerml".to_owned(),
            first: "*b.kerml".to_owned(),
            second: "a*.kerml".to_owned(),
        }
    );
}

/// A matched file is held to the rules of a written entry.
#[test]
fn a_match_that_is_not_a_valid_entry_is_an_error() {
    const FILES: &[&str] = &["a\\b.kerml"];
    assert_eq!(
        expand_problem(&["*.kerml"], FILES),
        ExpandProblem::Invalid {
            entry: "*.kerml".to_owned(),
            path: "a\\b.kerml".to_owned(),
            problem: PathProblem::Backslash,
        }
    );
}
