//! OpenAPI component names must be unique: utoipa keeps only the last schema registered under a
//! name, so two `Status` types would silently publish one of them for both (and clients generated
//! from the document decode the other wrongly). Rename one with `#[schema(as = OtherName)]`, or
//! `text_enum!(Name as OtherName { ... })`.
//!
//! utoipa offers no way to list the schemas it overwrote, so this scans the workspace sources for
//! every type deriving `ToSchema` (directly, through `cfg_attr`, or via `text_enum!`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("readable source directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `schema(as = Name)` or `schema(as = path::Name)` in an attribute line.
fn renamed(attribute: &str) -> Option<String> {
    let rest = attribute.split("schema(").nth(1)?.trim_start();
    let rest = rest
        .strip_prefix("as")?
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let path: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
        .collect();
    path.rsplit("::").next().map(str::to_owned)
}

fn identifier(text: &str) -> Option<String> {
    let name: String = text
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Schema name -> where it is defined.
fn schema_names(file: &Path) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(file).expect("readable source file");
    let mut found = Vec::new();
    let mut attributes: Vec<&str> = Vec::new();
    let mut in_text_enum = false;
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        let place = format!("{}:{}", file.display(), number + 1);
        if in_text_enum {
            // The first `Name {` or `Name as Other {` after `text_enum!(` (doc lines skipped).
            if line.starts_with("///") || line.starts_with("#[") || line.is_empty() {
                continue;
            }
            in_text_enum = false;
            let schema = match line.split_once(" as ") {
                Some((_, other)) => identifier(other),
                None => identifier(line),
            };
            if let Some(schema) = schema {
                found.push((schema, place));
            }
            continue;
        }
        if line.starts_with("text_enum!(") && !line.contains("macro_rules") {
            in_text_enum = true;
            continue;
        }
        if line.starts_with("#[") || line.starts_with("//") {
            attributes.push(line);
            continue;
        }
        let item = line
            .strip_prefix("pub struct ")
            .or_else(|| line.strip_prefix("pub enum "));
        if let Some(item) = item
            && attributes.iter().any(|a| a.contains("ToSchema"))
            && let Some(name) = identifier(item)
        {
            let schema = attributes.iter().find_map(|a| renamed(a)).unwrap_or(name);
            found.push((schema, place));
        }
        attributes.clear();
    }
    found
}

#[test]
fn openapi_schema_names_are_unique() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in &files {
        for (schema, place) in schema_names(file) {
            seen.entry(schema).or_default().push(place);
        }
    }
    assert!(
        seen.len() > 50,
        "the scan found too few schemas: {}",
        seen.len()
    );
    let duplicates: Vec<_> = seen.iter().filter(|(_, places)| places.len() > 1).collect();
    assert!(
        duplicates.is_empty(),
        "schema names used by more than one type (rename one with #[schema(as = ...)]): {duplicates:#?}"
    );
}
