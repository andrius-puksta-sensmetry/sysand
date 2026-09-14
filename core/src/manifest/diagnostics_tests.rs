// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: © 2026 Sysand contributors <opensource@sensmetry.com>

use serde::{Deserialize, Serialize};

use crate::manifest::diagnostics::{
    Diagnostic, KeyLocation, Position, ReadWarning, Report, Severity, missing_key,
};
use crate::model::MetamodelFamily;

#[derive(Debug, Deserialize, Serialize)]
struct Table {
    name: String,
    #[serde(flatten)]
    unknown: crate::manifest::diagnostics::UnknownKeys,
}

/// workspaces.md "Unknown keys": unknown keys are allowed for future
/// compatibility "but generate a warning upon read", so reading must keep
/// them rather than drop them (serde's default) or reject them
/// (`deny_unknown_fields`).
#[test]
fn unknown_keys_are_kept_as_warnings_not_errors() {
    let table: Table =
        toml_edit::de::from_str("name = \"a\"\nzzz = 1\naaa = { b = 2 }\n").expect("parses");

    assert_eq!(table.name, "a");
    assert_eq!(table.unknown.iter().collect::<Vec<_>>(), ["aaa", "zzz"]);
    assert!(!table.unknown.is_empty());

    let warnings = table
        .unknown
        .warnings(&KeyLocation::Table("project".to_owned()));
    assert_eq!(warnings.len(), 2);
    assert_eq!(warnings[0].to_string(), "unknown key `aaa` in `[project]`");
}

#[test]
fn unknown_keys_serializes_to_nothing() {
    let table: Table = toml_edit::de::from_str("name = \"a\"\nzzz = 1\n").expect("parses");
    let rendered = toml_edit::ser::to_string(&table).expect("serializes");

    assert_eq!(rendered, "name = \"a\"\n");
}

/// Appendix A rows 1 and 2: the unknown-key warning names the table, uses
/// a dedicated wording at the top level, and gains the "unsupported
/// metamodel" suffix when the project declares a metamodel this version
/// does not recognize.
#[test]
fn unknown_key_warning_texts() {
    let in_table = ReadWarning::UnknownKey {
        location: KeyLocation::Table("project".to_owned()),
        key: "shiny".to_owned(),
        unrecognized_metamodel: None,
    };
    assert_eq!(in_table.to_string(), "unknown key `shiny` in `[project]`");

    let top_level = ReadWarning::UnknownKey {
        location: KeyLocation::TopLevel,
        key: "shiny".to_owned(),
        unrecognized_metamodel: None,
    };
    assert_eq!(top_level.to_string(), "unknown top-level key `shiny`");

    let unsupported = ReadWarning::UnknownKey {
        location: KeyLocation::Table("project".to_owned()),
        key: "shiny".to_owned(),
        unrecognized_metamodel: Some("https://example.com/spec/Other/1".to_owned()),
    };
    assert_eq!(
        unsupported.to_string(),
        "unknown key `shiny` in `[project]`; this may be because the project uses a metamodel \
         this version of Sysand does not support: `https://example.com/spec/Other/1`"
    );
}

/// Appendix A row 17: `root` belongs to the Syside metamodel's schema.
#[test]
fn foreign_schema_key_warning_text() {
    let warning = ReadWarning::ForeignSchemaKey {
        location: KeyLocation::Table("project".to_owned()),
        key: "root".to_owned(),
        supported_by: MetamodelFamily::Syside,
    };
    assert_eq!(
        warning.to_string(),
        "`root` in `[project]` is only supported by the Syside metamodel and is ignored here"
    );
}

/// model-toml.md "Metamodel": "Any other value is a warning, not an error."
#[test]
fn unknown_metamodel_warning_text() {
    let warning = ReadWarning::UnknownMetamodel {
        location: KeyLocation::Table("project".to_owned()),
        value: "https://example.com/spec/Other/1".to_owned(),
    };
    assert_eq!(
        warning.to_string(),
        "`[project]` declares metamodel `https://example.com/spec/Other/1`, which this version \
         of Sysand does not recognize"
    );
}

/// Appendix A row 26 (workspaces.md "Source overrides").
#[test]
fn ignored_source_overrides_warning_text() {
    let warning = ReadWarning::IgnoredSourceOverrides {
        path: "member/sysand.toml".to_owned(),
    };
    assert_eq!(
        warning.to_string(),
        "`member/sysand.toml` is not at the workspace root, so it is ignored as a whole \
         (source overrides and `[[index]]` configuration alike)"
    );
}

#[test]
fn position_resolves_line_and_column_from_a_span() {
    let text = "manifest-version = 1\n\n[project]\nname = \"ä-project\"\nversion = 1\n";
    let span = |needle: &str| {
        let start = text.find(needle).expect("needle is in the text");
        start..start + needle.len()
    };

    let start = Position::from_span("model.toml", text, span("manifest-version"));
    assert_eq!((start.line, start.column), (1, 1));

    let project = Position::from_span("model.toml", text, span("[project]"));
    assert_eq!((project.line, project.column), (3, 1));

    let version_line = span("\nversion = 1");
    let version = Position::from_span("model.toml", text, version_line.start + 1..version_line.end);
    assert_eq!((version.line, version.column), (5, 1));

    // The column counts characters, not bytes: `ä` is two bytes, so the
    // `-` that follows it is the tenth character of its line.
    let inside = Position::from_span("model.toml", text, span("-project"));
    assert_eq!((inside.line, inside.column), (4, 10));
    assert_eq!(inside.span, Some(span("-project")));
}

#[test]
fn report_into_result_separates_errors_from_warnings() {
    let mut warnings_only = Report::new();
    warnings_only.push(Diagnostic::warning("something to know"));
    let validated = warnings_only.into_result(42).expect("warnings do not fail");
    assert_eq!(validated.value, 42);
    assert_eq!(validated.warnings.len(), 1);

    let mut with_error = Report::new();
    with_error.push(Diagnostic::warning("something to know"));
    with_error.push(Diagnostic::error("something fatal"));
    let report = with_error.into_result(42).expect_err("an error fails");
    assert!(report.has_errors());
    assert_eq!(report.errors().count(), 1);
    assert_eq!(report.warnings().count(), 1);
}

#[test]
fn report_display_lists_every_diagnostic() {
    let text = "a = 1\nname = 2\n";
    let mut report = Report::new();
    report.push(Diagnostic::error("first problem"));
    report.push(Diagnostic::error("second problem").at(Position::from_span(
        "manifest.toml",
        text,
        6..10,
    )));

    assert_eq!(
        report.to_string(),
        "first problem\nmanifest.toml:2:1: second problem"
    );
    assert_eq!(report.diagnostics().len(), 2);
    assert_eq!(report.diagnostics()[0].severity, Severity::Error);
}

#[test]
fn read_warnings_become_warning_diagnostics() {
    let warning = ReadWarning::UnknownMetamodel {
        location: KeyLocation::Table("project".to_owned()),
        value: "https://example.com/x".to_owned(),
    };
    let positioned = warning.clone().at(Position::at("model.toml", 4, 2));
    assert_eq!(positioned.severity, Severity::Warning);
    assert_eq!(
        positioned.position.as_ref().map(|p| (p.line, p.column)),
        Some((4, 2))
    );

    let plain = Diagnostic::from(warning);
    assert_eq!(plain.severity, Severity::Warning);
    assert!(plain.position.is_none());
}

/// Appendix A row 1, for both manifest formats: a `model.toml` names the
/// table, a json manifest names the file.
#[test]
fn missing_key_names_its_location_and_the_metamodel() {
    assert_eq!(
        missing_key("name", &KeyLocation::Table("project".to_owned()), None).to_string(),
        "missing key `name` in `[project]`"
    );
    assert_eq!(
        missing_key("index", &KeyLocation::File(".meta.json".to_owned()), None).to_string(),
        "missing key `index` in `.meta.json`"
    );
    assert_eq!(
        missing_key(
            "index",
            &KeyLocation::File(".meta.json".to_owned()),
            Some("https://example.com/x")
        )
        .to_string(),
        "missing key `index` in `.meta.json`; this may be because the project uses a metamodel \
         this version of Sysand does not support: `https://example.com/x`"
    );
}

/// Appendix A row 1 for a usage entry: an array of tables is written with
/// double brackets, so naming it with the single-bracket form of a plain
/// table would send the reader looking for a table that is not there.
#[test]
fn array_tables_are_named_with_double_brackets() {
    assert_eq!(
        missing_key(
            "name",
            &KeyLocation::ArrayTable("inline-usages".to_owned()),
            None
        )
        .to_string(),
        "missing key `name` in `[[inline-usages]]`"
    );
}

/// A diagnostic about a key that is *absent* often has nowhere to point,
/// and a manifest that was not parsed from text has no spans at all.
/// Neither may invent a position.
#[test]
fn a_diagnostic_without_a_position_is_unchanged() {
    let position = Position::at("model.toml", 3, 5);
    assert_eq!(Diagnostic::error("x").at_opt(None), Diagnostic::error("x"));
    assert_eq!(
        Diagnostic::error("x").at_opt(Some(position.clone())),
        Diagnostic::error("x").at(position)
    );
}

/// The checks that produce diagnostics run in field order, and unknown
/// keys are found alphabetically; neither is the order the reader reads
/// the file in.
#[test]
fn a_report_sorts_by_position() {
    let text = "one\ntwo\nthree\n";
    let mut report = Report::new();
    report.push(Diagnostic::error("third").at(Position::from_span("f", text, 8..13)));
    report.push(Diagnostic::error("first").at(Position::from_span("f", text, 0..3)));
    report.push(Diagnostic::warning("second").at(Position::from_span("f", text, 4..7)));
    report.sort_by_position();

    let messages: Vec<&str> = report
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert_eq!(messages, ["first", "second", "third"]);
}

/// A diagnostic with no position cannot be placed among the ones that
/// have one, and two diagnostics at the same key keep the order the
/// checks found them in.
#[test]
fn a_positionless_diagnostic_sorts_last() {
    let text = "one\ntwo\n";
    let mut report = Report::new();
    report.push(Diagnostic::error("nowhere"));
    report.push(Diagnostic::error("same place, first"));
    report.push(Diagnostic::error("later").at(Position::from_span("f", text, 4..7)));
    report.push(Diagnostic::error("earlier").at(Position::from_span("f", text, 0..3)));
    // Two diagnostics about one key.
    report.push(Diagnostic::error("same place, second"));
    report.sort_by_position();

    let messages: Vec<&str> = report
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "earlier",
            "later",
            "nowhere",
            "same place, first",
            "same place, second"
        ]
    );
}
