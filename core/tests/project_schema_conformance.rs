// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: © 2026 Sysand contributors <opensource@sensmetry.com>

//! Conformance of `.project.json` and `.meta.json` deserialization with the
//! KerML interchange project JSON schema (`Project` and `Meta` in
//! `kpar_json_schema.json`).
//!
//! The schema constrains structure only -- types, required properties and
//! the shape of `usage`/`index`/`checksum` entries -- and leaves
//! `additionalProperties` at its default, so unknown properties are allowed
//! everywhere. It says nothing about the *content* of the strings beyond the
//! `format` annotations on `website`, `usage[].resource`, `metamodel` and
//! `created`.
//!
//! Sysand splits those two concerns: `serde` deserialization into the `Raw`
//! types should correspond to the schema's structure, and `validate()`
//! applies Sysand's own, deliberately stricter rules on top. The tests below
//! pin both layers:
//!
//! - [`schema_valid_documents_deserialize`] and
//!   [`schema_invalid_documents_are_rejected`] check the structural layer
//!   against the schema and pass today.
//! - [`stricter_than_schema_but_deserialized`] records, as a single list,
//!   every place `validate()` rejects a document the schema accepts. These
//!   are intentional; the test exists so that widening or narrowing a rule
//!   is a visible change.
//! - The `#[should_panic]` tests at the end are the three structural
//!   divergences found. They pass while the divergence is open and fail once
//!   it is fixed, at which point the case belongs in the table above.

use sysand_core::model::{InterchangeProjectInfoRaw, InterchangeProjectMetadataRaw};

/// A `.project.json` the schema accepts must deserialize.
fn assert_info_deserializes(label: &str, json: &str) {
    if let Err(e) = serde_json::from_str::<InterchangeProjectInfoRaw>(json) {
        panic!("schema-valid `.project.json` ({label}) was rejected: {e}\n{json}");
    }
}

/// A `.meta.json` the schema accepts must deserialize.
fn assert_meta_deserializes(label: &str, json: &str) {
    if let Err(e) = serde_json::from_str::<InterchangeProjectMetadataRaw>(json) {
        panic!("schema-valid `.meta.json` ({label}) was rejected: {e}\n{json}");
    }
}

/// A `.project.json` the schema rejects must not deserialize.
fn assert_info_rejected(label: &str, json: &str) {
    assert!(
        serde_json::from_str::<InterchangeProjectInfoRaw>(json).is_err(),
        "schema-invalid `.project.json` ({label}) was accepted\n{json}"
    );
}

/// A `.meta.json` the schema rejects must not deserialize.
fn assert_meta_rejected(label: &str, json: &str) {
    assert!(
        serde_json::from_str::<InterchangeProjectMetadataRaw>(json).is_err(),
        "schema-invalid `.meta.json` ({label}) was accepted\n{json}"
    );
}

/// Everything the schema permits deserializes, including the parts Sysand
/// has no use for: unknown properties at every level, and string contents
/// that carry no meaning to Sysand.
#[test]
fn schema_valid_documents_deserialize() {
    // `Project`: `name` and `version` are the only required properties.
    assert_info_deserializes("minimal", r#"{"name":"a","version":"1.0.0"}"#);
    assert_info_deserializes("empty strings", r#"{"name":"","version":""}"#);
    assert_info_deserializes(
        "every optional property",
        r#"{"name":"a","version":"1.0.0","description":"d","license":"MIT",
            "maintainer":["m"],"website":"https://example.com","topic":["t"],"usage":[]}"#,
    );
    assert_info_deserializes(
        "unknown top-level property",
        r#"{"name":"a","version":"1.0.0","vendorExtension":{"x":1}}"#,
    );
    assert_info_deserializes(
        "usage with `resource` only",
        r#"{"name":"a","version":"1.0.0","usage":[{"resource":"https://example.com/x.kpar"}]}"#,
    );
    assert_info_deserializes(
        "usage with a version constraint",
        r#"{"name":"a","version":"1.0.0",
            "usage":[{"resource":"https://example.com/x.kpar","versionConstraint":">=1"}]}"#,
    );
    assert_info_deserializes(
        "usage with an unknown property",
        r#"{"name":"a","version":"1.0.0",
            "usage":[{"resource":"https://example.com/x.kpar","note":"hi"}]}"#,
    );

    // `Meta`: `index` and `created` are the only required properties.
    assert_meta_deserializes(
        "minimal",
        r#"{"index":{},"created":"2026-01-01T00:00:00Z"}"#,
    );
    assert_meta_deserializes(
        "every optional property",
        r#"{"index":{"A":"a.sysml"},"created":"2026-01-01T00:00:00Z",
            "metamodel":"https://www.omg.org/spec/KerML/20250201",
            "includesDerived":true,"includesImplied":false,
            "checksum":{"a.sysml":{"algorithm":"SHA256",
            "value":"0000000000000000000000000000000000000000000000000000000000000000"}}}"#,
    );
    assert_meta_deserializes(
        "unknown top-level property",
        r#"{"index":{},"created":"2026-01-01T00:00:00Z","vendorExtension":[1,2]}"#,
    );
    assert_meta_deserializes(
        "checksum entry with an unknown property",
        r#"{"index":{},"created":"2026-01-01T00:00:00Z",
            "checksum":{"a.sysml":{"algorithm":"NONE","value":"","note":1}}}"#,
    );
    assert_meta_deserializes(
        "empty index key",
        r#"{"index":{"":"a.sysml"},"created":"2026-01-01T00:00:00Z"}"#,
    );
}

/// Structure the schema forbids is refused before validation: missing
/// required properties, and values of the wrong JSON type.
#[test]
fn schema_invalid_documents_are_rejected() {
    assert_info_rejected("missing `version`", r#"{"name":"a"}"#);
    assert_info_rejected("missing `name`", r#"{"version":"1.0.0"}"#);
    assert_info_rejected("`version` is a number", r#"{"name":"a","version":1}"#);
    assert_info_rejected("`name` is null", r#"{"name":null,"version":"1.0.0"}"#);
    assert_info_rejected(
        "`description` is a number",
        r#"{"name":"a","version":"1.0.0","description":7}"#,
    );
    assert_info_rejected(
        "`maintainer` is a string",
        r#"{"name":"a","version":"1.0.0","maintainer":"m"}"#,
    );
    assert_info_rejected(
        "`topic` items are numbers",
        r#"{"name":"a","version":"1.0.0","topic":[1]}"#,
    );
    assert_info_rejected(
        "`usage` is an object",
        r#"{"name":"a","version":"1.0.0","usage":{"resource":"urn:x"}}"#,
    );
    assert_info_rejected(
        "usage without `resource`",
        r#"{"name":"a","version":"1.0.0","usage":[{"versionConstraint":">=1"}]}"#,
    );
    assert_info_rejected(
        "usage `resource` is a number",
        r#"{"name":"a","version":"1.0.0","usage":[{"resource":1}]}"#,
    );

    assert_meta_rejected("missing `index`", r#"{"created":"2026-01-01T00:00:00Z"}"#);
    assert_meta_rejected("missing `created`", r#"{"index":{}}"#);
    assert_meta_rejected(
        "`index` is an array",
        r#"{"index":[],"created":"2026-01-01T00:00:00Z"}"#,
    );
    assert_meta_rejected(
        "`index` value is a number",
        r#"{"index":{"A":1},"created":"2026-01-01T00:00:00Z"}"#,
    );
    assert_meta_rejected("`created` is a number", r#"{"index":{},"created":1}"#);
    assert_meta_rejected(
        "`includesDerived` is a string",
        r#"{"index":{},"created":"2026-01-01T00:00:00Z","includesDerived":"true"}"#,
    );
    assert_meta_rejected(
        "`checksum` is an array",
        r#"{"index":{},"created":"2026-01-01T00:00:00Z","checksum":[]}"#,
    );
    assert_meta_rejected(
        "checksum entry without `algorithm`",
        r#"{"index":{},"created":"2026-01-01T00:00:00Z","checksum":{"a.sysml":{"value":"00"}}}"#,
    );
    assert_meta_rejected(
        "checksum entry without `value`",
        r#"{"index":{},"created":"2026-01-01T00:00:00Z",
            "checksum":{"a.sysml":{"algorithm":"SHA256"}}}"#,
    );
    assert_meta_rejected(
        "checksum `value` is a number",
        r#"{"index":{},"created":"2026-01-01T00:00:00Z",
            "checksum":{"a.sysml":{"algorithm":"SHA256","value":0}}}"#,
    );
}

/// Documents the schema accepts but Sysand's own validation refuses. These
/// are deliberate: the schema types these fields as plain strings, while
/// Sysand needs a version it can order, a constraint it can solve, an IRI it
/// can resolve and a path it can open. Listing them in one place means a
/// change to any rule shows up as a change to this test.
///
/// The one that bites: `version` is a plain string to the schema, and
/// `sysand init --no-semver --version 1.0` deliberately writes one, but
/// `sysand lock` then refuses the project it just created. `sysand build`
/// only warns about the same project.
#[test]
fn stricter_than_schema_but_deserialized() {
    let info_cases = [
        ("version is not SemVer", r#"{"name":"a","version":"1.0"}"#),
        (
            "website is not an IRI",
            r#"{"name":"a","version":"1.0.0","website":"not an iri"}"#,
        ),
        (
            "usage resource is a relative reference, not an IRI",
            r#"{"name":"a","version":"1.0.0","usage":[{"resource":"../sibling"}]}"#,
        ),
        (
            "usage version constraint is not a Cargo-style requirement",
            r#"{"name":"a","version":"1.0.0",
                "usage":[{"resource":"urn:x","versionConstraint":"latest"}]}"#,
        ),
        (
            "pkg:sysand usage is not normalized",
            r#"{"name":"a","version":"1.0.0","usage":[{"resource":"pkg:sysand/Acme/Other"}]}"#,
        ),
        (
            "pkg:sysand usage has three segments",
            r#"{"name":"a","version":"1.0.0","usage":[{"resource":"pkg:sysand/a/b/c"}]}"#,
        ),
    ];
    for (label, json) in info_cases {
        let raw = serde_json::from_str::<InterchangeProjectInfoRaw>(json)
            .unwrap_or_else(|e| panic!("schema-valid `.project.json` ({label}) was rejected: {e}"));
        assert!(
            raw.validate().is_err(),
            "`.project.json` ({label}) is no longer rejected by validation; \
             if that is intended, move it to `schema_valid_documents_deserialize`"
        );
    }

    let meta_cases = [
        (
            "index path has a `./` prefix",
            r#"{"index":{"A":"./a.sysml"},"created":"2026-01-01T00:00:00Z"}"#,
        ),
        (
            "index path escapes the project",
            r#"{"index":{"A":"../a.sysml"},"created":"2026-01-01T00:00:00Z"}"#,
        ),
        (
            "index path is absolute",
            r#"{"index":{"A":"/a.sysml"},"created":"2026-01-01T00:00:00Z"}"#,
        ),
        (
            "index path is empty",
            r#"{"index":{"A":""},"created":"2026-01-01T00:00:00Z"}"#,
        ),
        (
            "index path uses Windows separators",
            r#"{"index":{"A":"src\\a.sysml"},"created":"2026-01-01T00:00:00Z"}"#,
        ),
        (
            "created has no time component",
            r#"{"index":{},"created":"2026-01-01"}"#,
        ),
        (
            "metamodel is not an IRI",
            r#"{"index":{},"created":"2026-01-01T00:00:00Z","metamodel":"nope nope"}"#,
        ),
        (
            "checksum algorithm is not one KerML lists",
            r#"{"index":{},"created":"2026-01-01T00:00:00Z",
                "checksum":{"a.sysml":{"algorithm":"CRC32","value":"00000000"}}}"#,
        ),
        (
            // Listed in neither KerML 10.3 note 6 nor `KerMlChecksumAlg`,
            // unlike SHA-384 -- see the TODO on that enum.
            "checksum algorithm is SHA512",
            r#"{"index":{},"created":"2026-01-01T00:00:00Z",
                "checksum":{"a.sysml":{"algorithm":"SHA512","value":"00"}}}"#,
        ),
        (
            "checksum algorithm differs in case",
            r#"{"index":{},"created":"2026-01-01T00:00:00Z",
                "checksum":{"a.sysml":{"algorithm":"sha256","value":"00"}}}"#,
        ),
        (
            "checksum digest has the wrong length for its algorithm",
            r#"{"index":{},"created":"2026-01-01T00:00:00Z",
                "checksum":{"a.sysml":{"algorithm":"SHA256","value":"00"}}}"#,
        ),
    ];
    for (label, json) in meta_cases {
        let raw = serde_json::from_str::<InterchangeProjectMetadataRaw>(json)
            .unwrap_or_else(|e| panic!("schema-valid `.meta.json` ({label}) was rejected: {e}"));
        assert!(
            raw.validate().is_err(),
            "`.meta.json` ({label}) is no longer rejected by validation; \
             if that is intended, move it to `schema_valid_documents_deserialize`"
        );
    }
}

/// The schema types `Project` as `"type": "object"`, but the derived
/// `Deserialize` also accepts the struct's fields as a positional JSON
/// array, so a `.project.json` that is an array is read as a project.
/// `sysand info` prints the result without complaint.
///
/// Beyond accepting a document the schema forbids, this makes field order
/// part of the accepted input: adding a field to
/// `InterchangeProjectInfoG` silently changes which arrays parse.
#[test]
#[should_panic(expected = "schema-invalid `.project.json` (array form) was accepted")]
fn project_json_must_be_a_json_object() {
    assert_info_rejected(
        "array form",
        r#"["a",null,null,"1.0.0",null,[],null,[],[]]"#,
    );
}

/// As [`project_json_must_be_a_json_object`], for `Meta`.
#[test]
#[should_panic(expected = "schema-invalid `.meta.json` (array form) was accepted")]
fn meta_json_must_be_a_json_object() {
    assert_meta_rejected(
        "array form",
        r#"[{},"2026-01-01T00:00:00Z",null,null,null,null]"#,
    );
}

/// The schema types `usage[].versionConstraint` as `"type": "string"`, which
/// does not admit `null`. Sysand models it as `Option<String>`, so an
/// explicit `null` is read as "no constraint" rather than refused.
///
/// Harmless on its own -- Sysand writes the property out only when it is
/// set -- but it means Sysand accepts input a conforming producer would not
/// emit and a conforming consumer would reject.
#[test]
#[should_panic(expected = "schema-invalid `.project.json` (null version constraint) was accepted")]
fn usage_version_constraint_must_not_be_null() {
    assert_info_rejected(
        "null version constraint",
        r#"{"name":"a","version":"1.0.0","usage":[{"resource":"urn:x","versionConstraint":null}]}"#,
    );
}
