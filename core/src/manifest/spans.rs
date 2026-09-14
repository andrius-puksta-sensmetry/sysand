// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: © 2026 Sysand contributors <opensource@sensmetry.com>

//! Where every key of a `model.toml` is, so that a diagnostic can point
//! at the one that caused it.
//!
//! D10 of the phase-1 plan: positions come from `Key::span` of a parsed
//! `toml_edit::Document`. The document is consumed by the deserializer
//! (`toml_edit::de::Deserializer::from`), which is why the spans are
//! harvested into an owned index first:
//!
//! ```ignore
//! let document = toml_edit::Document::parse(text)?;
//! let positions = Positions::new(file, text, SpanIndex::harvest(&document));
//! let raw = ModelTomlRaw::deserialize(toml_edit::de::Deserializer::from(document))?;
//! ```
//!
//! The text is parsed once: the deserializer keeps it, so a failure
//! inside serde still reports a span of its own.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::ops::Range;

use toml_edit::{Document, Item, Table, Value};

use crate::manifest::diagnostics::Position;

/// The span of every key of a parsed document, addressed by the path of
/// key segments that leads to it.
///
/// Segments rather than one dotted string, because TOML lets a quoted key
/// contain a dot: `"a.b" = 1` and `[a]` with `b = 2` are different keys
/// that may both appear in one file, and both would flatten to `a.b`.
///
/// An element of an array of tables is addressed by its index
/// (`["inline-usages", "0"]`) and holds the span of its `[[...]]` header,
/// which is what a diagnostic about the entry as a whole points at.
/// Elements of an array that are not tables are not indexed: a diagnostic
/// about one points at the key of the array, which is where the reader
/// can act.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpanIndex(BTreeMap<Vec<String>, Range<usize>>);

impl SpanIndex {
    /// Walk a parsed document, recording where each key is.
    pub fn harvest<S: AsRef<str>>(document: &Document<S>) -> Self {
        let mut index = Self::default();
        let mut path = Vec::new();
        index.walk_table(&mut path, document.as_table());
        index
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The span recorded for exactly this path, if there is one.
    pub fn get(&self, path: &[&str]) -> Option<&Range<usize>> {
        let path: Vec<String> = path.iter().map(|segment| (*segment).to_owned()).collect();
        self.0.get(&path)
    }

    /// The span of this path, or of the longest prefix of it that has
    /// one. A key that is missing has no span of its own, so a diagnostic
    /// about it points at the table or entry it is missing from.
    pub fn get_near(&self, path: &[&str]) -> Option<&Range<usize>> {
        (0..=path.len())
            .rev()
            .find_map(|length| self.get(&path[..length]))
    }

    /// First writes win: a path that is somehow reached twice keeps the
    /// span found first, so an unforeseen collision costs precision
    /// rather than correctness.
    fn insert(&mut self, path: &[String], span: Option<Range<usize>>) {
        let Some(span) = span else {
            return;
        };
        if let Entry::Vacant(entry) = self.0.entry(path.to_vec()) {
            entry.insert(span);
        }
    }

    fn walk_table(&mut self, path: &mut Vec<String>, table: &Table) {
        for (key, item) in table {
            let Some((key, _)) = table.get_key_value(key) else {
                continue;
            };
            path.push(key.get().to_owned());
            self.insert(path, key.span());
            self.walk_item(path, item);
            path.pop();
        }
    }

    fn walk_item(&mut self, path: &mut Vec<String>, item: &Item) {
        match item {
            Item::Table(table) => self.walk_table(path, table),
            Item::ArrayOfTables(tables) => {
                for (index, table) in tables.iter().enumerate() {
                    path.push(index.to_string());
                    // The `[[...]]` header, not the entry's body.
                    self.insert(path, table.span());
                    self.walk_table(path, table);
                    path.pop();
                }
            }
            Item::Value(value) => self.walk_value(path, value),
            Item::None => {}
        }
    }

    fn walk_value(&mut self, path: &mut Vec<String>, value: &Value) {
        // An inline table is the same table written differently, so it is
        // indexed the same way; every other value is a leaf, and the key
        // that holds it is already indexed.
        let Value::InlineTable(table) = value else {
            return;
        };
        for (key, value) in table {
            let Some((key, _)) = table.get_key_value(key) else {
                continue;
            };
            path.push(key.get().to_owned());
            self.insert(path, key.span());
            self.walk_value(path, value);
            path.pop();
        }
    }
}

/// A [`SpanIndex`] together with what turns a span into a
/// [`Position`]: the name of the file and its text.
///
/// Borrowing the text rather than the document is what lets the document
/// be handed to the deserializer; nothing that escapes into a report
/// borrows anything.
#[derive(Clone, Debug)]
pub struct Positions<'a> {
    file: String,
    text: &'a str,
    spans: SpanIndex,
}

impl<'a> Positions<'a> {
    pub fn new(file: impl Into<String>, text: &'a str, spans: SpanIndex) -> Self {
        Self {
            file: file.into(),
            text,
            spans,
        }
    }

    /// For a manifest that was not read from text -- one a command built
    /// itself, or a json project converted into the toml model. Every
    /// lookup yields `None`, so the validation code needs no second path
    /// for the case.
    pub fn without_spans(file: impl Into<String>) -> Positions<'static> {
        Positions {
            file: file.into(),
            text: "",
            spans: SpanIndex::default(),
        }
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    /// Where this key is, if it is in the document.
    pub fn at(&self, path: &[&str]) -> Option<Position> {
        let span = self.spans.get(path)?;
        Some(Position::from_span(
            self.file.clone(),
            self.text,
            span.clone(),
        ))
    }

    /// Where this key is, or the innermost thing on its path that is in
    /// the document. This is what a diagnostic about a *missing* key
    /// uses.
    pub fn near(&self, path: &[&str]) -> Option<Position> {
        let span = self.spans.get_near(path)?;
        Some(Position::from_span(
            self.file.clone(),
            self.text,
            span.clone(),
        ))
    }
}

#[cfg(test)]
#[path = "./spans_tests.rs"]
mod tests;
