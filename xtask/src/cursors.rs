//! `cargo xtask cursors`: does the cursor manifest (`crates/tools/src/cursors.toml`) cover every
//! `Cursor` value?
//!
//! Only warns: a cursor without an entry still works in the app (it falls back), it just never
//! reaches designers. Development never stops for a missing cursor.

use std::collections::BTreeSet;
use std::path::Path;

const ENUM_FILE: &str = "crates/tools/src/lib.rs";
const MANIFEST: &str = "crates/tools/src/cursors.toml";

pub fn run(root: &Path) -> Result<(), String> {
    let read = |p: &str| std::fs::read_to_string(root.join(p)).map_err(|e| format!("{p}: {e}"));
    let warnings = check(&read(ENUM_FILE)?, &read(MANIFEST)?);
    if warnings.is_empty() {
        println!("cursors: every Cursor has an entry in {MANIFEST}");
    }
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    Ok(())
}

/// Warnings for `Cursor` variants in `enum_src` and `[[cursor]]` entries in `manifest` that don't
/// match, and for entries that repeat an id or lack a required field.
pub fn check(enum_src: &str, manifest: &str) -> Vec<String> {
    let variants = variants(enum_src);
    let entries = entries(manifest);
    let mut out = vec![];
    if variants.is_empty() {
        out.push(format!("no `pub enum Cursor` found in {ENUM_FILE}"));
    }
    let mut ids = BTreeSet::new();
    for e in &entries {
        let name = e.id.as_deref().unwrap_or("?");
        match &e.id {
            None => out.push(format!("{MANIFEST}:{}: a [[cursor]] without an id", e.line)),
            Some(id) if !ids.insert(id.as_str()) => out.push(format!("{MANIFEST}:{}: {id} is listed twice", e.line)),
            Some(id) if !variants.contains(id) => out.push(format!("{MANIFEST}:{}: {id} is not a Cursor value", e.line)),
            Some(_) => {}
        }
        for field in ["shown_when", "hotspot"] {
            if !e.fields.contains(field) {
                out.push(format!("{MANIFEST}:{}: {name} has no {field}", e.line));
            }
        }
    }
    for v in variants.iter().filter(|v| !ids.contains(v.as_str())) {
        out.push(format!("Cursor::{v} has no entry in {MANIFEST} (designers won't see it; see CLAUDE.md › Adding a cursor)"));
    }
    out
}

/// The variant names of `pub enum Cursor` in `src`.
fn variants(src: &str) -> BTreeSet<String> {
    src.lines()
        .skip_while(|l| !l.starts_with("pub enum Cursor {"))
        .skip(1)
        .take_while(|l| !l.starts_with('}'))
        .map(str::trim)
        .filter(|l| l.starts_with(|c: char| c.is_ascii_uppercase()))
        .map(|l| l.chars().take_while(|c| c.is_ascii_alphanumeric()).collect())
        .collect()
}

/// One `[[cursor]]` table: its first line (1-based), its `id` and the keys it sets.
struct Entry {
    line: usize,
    id: Option<String>,
    fields: BTreeSet<String>,
}

/// The `[[cursor]]` tables of `manifest`, read line by line (keys at the start of a line).
fn entries(manifest: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = vec![];
    let mut in_cursor = false;
    for (i, raw) in manifest.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with('[') && !line.starts_with("[\"") {
            in_cursor = line == "[[cursor]]";
            if in_cursor {
                out.push(Entry { line: i + 1, id: None, fields: BTreeSet::new() });
            }
            continue;
        }
        let (Some(e), Some((key, value))) = (out.last_mut().filter(|_| in_cursor), raw.split_once('=')) else { continue };
        if raw.starts_with(char::is_whitespace) || key.trim().contains(' ') {
            continue;
        }
        let key = key.trim();
        if key == "id" {
            e.id = Some(value.trim().trim_matches('"').to_string());
        }
        e.fields.insert(key.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENUM: &str = "pub enum Cursor {\n    #[default]\n    Arrow,\n    /// Doc.\n    RotateToward(u8),\n    Pen,\n}\n";

    fn entry(id: &str) -> String {
        format!("[[cursor]]\nid = \"{id}\"\nshown_when = [\n    \"One.\",\n]\nhotspot = \"Tip.\"\n\n")
    }

    #[test]
    fn a_complete_manifest_has_no_warnings() {
        let m = [entry("Arrow"), entry("RotateToward"), entry("Pen"), "[[not_needed]]\nwhat = \"x\"\n".into()].concat();
        assert_eq!(check(ENUM, &m), Vec::<String>::new());
    }

    #[test]
    fn warns_about_missing_unknown_repeated_and_incomplete_entries() {
        let m = [entry("Arrow"), entry("Arrow"), entry("Gone"), "[[cursor]]\nid = \"Pen\"\n".into()].concat();
        let w = check(ENUM, &m).join("\n");
        assert!(w.contains("Arrow is listed twice"), "{w}");
        assert!(w.contains("Gone is not a Cursor value"), "{w}");
        assert!(w.contains("Pen has no shown_when") && w.contains("Pen has no hotspot"), "{w}");
        assert!(w.contains("Cursor::RotateToward has no entry"), "{w}");
    }
}
