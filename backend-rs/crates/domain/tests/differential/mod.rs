//! Golden vectors generated from the Python backend (`fixtures/differential/generate.py`).
//! The ports of `allocate`, `split_items`, `settle_up`, `to_base`, the tallies, pact settlement
//! and the Santa shifts must reproduce them exactly.
#![allow(dead_code, clippy::unwrap_used, clippy::missing_panics_doc)] // used by later test files

use std::path::PathBuf;

use serde_json::Value;

pub const FILES: [&str; 10] = [
    "allocate",
    "split_items",
    "settle_up",
    "to_base",
    "breakdown_to_base",
    "format_amount",
    "schulze",
    "picks",
    "pact_settle",
    "santa_shifts",
];

/// The cases of `fixtures/differential/<name>.json`.
pub fn cases(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/differential")
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let document: Value = serde_json::from_str(&text).unwrap();
    document["cases"].as_array().unwrap().clone()
}

/// A `[[id, value], ...]` pair list as `(id, value)` tuples.
pub fn pairs(value: &Value) -> Vec<(i64, i64)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| (pair[0].as_i64().unwrap(), pair[1].as_i64().unwrap()))
        .collect()
}
