// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: © 2026 Sysand contributors <opensource@sensmetry.com>

//! The `model.toml` manifest: its value types, the diagnostics reading one
//! produces, and (later) the reader and the writers.
//!
//! Everything in this module is free of Cargo features, because every
//! consumer needs it: the CLI, the bindings that have no filesystem, and
//! the KPAR view a build derives from a manifest.

pub mod common;
pub mod diagnostics;
pub mod inherit;
pub mod reader;
pub mod serde_support;
pub mod spans;
pub mod usage;
pub mod workspace_table;
