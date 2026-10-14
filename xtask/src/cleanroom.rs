//! `cargo xtask cleanroom`: nothing Adobe-authored or Adobe-made in the repository (CLAUDE.md ›
//! Clean-room). Scans every tracked (and untracked, not ignored) file and fails on:
//!
//! * an Adobe copyright notice in a text file (`Copyright 1990-2023 Adobe`, `© Adobe Systems
//!   Incorporated`): the sign of a pasted procset, prolog, sample file or other Adobe text. Only
//!   the files in [`ALLOWED`] may carry one: credits for Adobe's open-source work, kept verbatim.
//! * a fixture an Adobe application made: creator metadata naming one (XMP `CreatorTool`, a PDF
//!   `/Creator` or `/Producer`, a PostScript `%%Creator:` line, the generator comment of an
//!   exported SVG), and in binary files the application's name itself (image software tags).
//!   Rust sources aren't checked for this: importers and their tests name these headers to
//!   recognise them, in hand-written input.

use std::path::Path;

/// Files that may carry an Adobe copyright notice, with why.
const ALLOWED: &[(&str, &str)] = &[
    ("NOTICE", "credits Adobe's CMap resources (BSD 3-Clause) and open-source fonts"),
    ("assets/fonts/OFL-SourceSans3.txt", "Source Sans 3's SIL Open Font License, kept verbatim"),
    ("assets/fonts/OFL-SourceSerif4.txt", "Source Serif 4's SIL Open Font License, kept verbatim"),
];

/// This file spells out what it looks for.
const SELF: &str = "xtask/src/cleanroom.rs";

/// Creator metadata naming an Adobe application, as it appears in files those applications write.
const CREATOR_MARKERS: &[&str] = &[
    "%%Creator: Adobe",
    "%%Creator:Adobe",
    "/Creator (Adobe",
    "/Creator(Adobe",
    "/Producer (Adobe",
    "/Producer(Adobe",
    "Generator: Adobe",
    "Acrobat Distiller",
    "Adobe PDF Library",
];

/// Application names that, inside a binary file, say which program wrote it.
const APP_NAMES: &[&str] = &["Adobe Photoshop", "Adobe Illustrator", "Adobe InDesign", "Adobe Acrobat", "Adobe Fireworks"];

/// How far after `xmp:CreatorTool` its value can start (attribute or element form).
const XMP_VALUE_WINDOW: usize = 40;

pub fn run(root: &Path) -> Result<(), String> {
    let files = crate::repo_files(root)?;
    let mut hits = vec![];
    for path in files.iter().filter(|p| p.as_str() != SELF) {
        let Ok(bytes) = std::fs::read(root.join(path)) else { continue };
        hits.extend(scan(path, &bytes));
    }
    if hits.is_empty() {
        println!("cleanroom: no Adobe copyright notices or Adobe-made fixtures");
        return Ok(());
    }
    Err(format!(
        "{} clean-room problem(s) (CLAUDE.md › Clean-room: write test inputs from scratch from the public specifications; never copy Adobe-authored text):\n  {}",
        hits.len(),
        hits.join("\n  ")
    ))
}

/// The problems in one file, as `path:line: what` (`path: what` for binary files).
pub fn scan(path: &str, bytes: &[u8]) -> Vec<String> {
    let text = std::str::from_utf8(bytes).ok().filter(|t| !t.contains('\0'));
    let mut out = vec![];
    if let Some(text) = text
        && !ALLOWED.iter().any(|(p, _)| *p == path)
    {
        for (i, line) in text.lines().enumerate() {
            if has_adobe_notice(line) {
                out.push(format!("{path}:{}: an Adobe copyright notice: {}", i + 1, short(line)));
            }
        }
    }
    if path.ends_with(".rs") {
        return out;
    }
    let mut found: Vec<&str> = CREATOR_MARKERS.iter().copied().filter(|m| contains(bytes, m.as_bytes())).collect();
    if xmp_creator_is_adobe(bytes) {
        found.push("xmp:CreatorTool naming Adobe");
    }
    if text.is_none() {
        found.extend(APP_NAMES.iter().copied().filter(|m| contains(bytes, m.as_bytes())));
    }
    if !found.is_empty() {
        out.push(format!("{path}: made with an Adobe application ({})", found.join(", ")));
    }
    out
}

/// Does `line` hold a copyright notice whose holder is Adobe: a copyright mark, then optional
/// marks and years, then "Adobe"?
fn has_adobe_notice(line: &str) -> bool {
    let lower = line.to_lowercase();
    ["copyright", "©", "(c)"].iter().any(|mark| {
        lower.match_indices(mark).any(|(i, m)| {
            let rest = lower[i + m.len()..].trim_start_matches(|c: char| c.is_whitespace() || c.is_ascii_digit() || "-–,©".contains(c));
            let rest =
                rest.strip_prefix("(c)").unwrap_or(rest).trim_start_matches(|c: char| c.is_whitespace() || c.is_ascii_digit() || "-–,".contains(c));
            rest.starts_with("adobe")
        })
    })
}

/// Does an `xmp:CreatorTool` value start with "Adobe"?
fn xmp_creator_is_adobe(bytes: &[u8]) -> bool {
    const KEY: &[u8] = b"xmp:CreatorTool";
    bytes.windows(KEY.len()).enumerate().filter(|(_, w)| *w == KEY).any(|(i, _)| {
        let after = bytes.get(i + KEY.len()..).unwrap_or_default();
        let window = after.get(..XMP_VALUE_WINDOW.min(after.len())).unwrap_or_default();
        let start = window.iter().position(|b| !matches!(b, b'=' | b'"' | b'\'' | b'>' | b' ' | b'\n' | b'\r' | b'\t'));
        start.is_some_and(|s| window.get(s..).is_some_and(|v| v.starts_with(b"Adobe")))
    })
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn short(s: &str) -> String {
    let s = s.trim();
    match s.char_indices().nth(100) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_adobe_copyright_notices_in_text() {
        for notice in [
            "% Copyright 1990-2023 Adobe. All rights reserved.",
            "(c) 1987 Adobe Systems Incorporated",
            "© Adobe",
            "Copyright (c) 1985, 1987 Adobe Systems",
        ] {
            assert_eq!(scan("crates/eps/procset.ps", notice.as_bytes()).len(), 1, "{notice}");
        }
        assert_eq!(scan("crates/eps/src/lib.rs", b"// Copyright 1999 Adobe Systems Incorporated\n").len(), 1, "Rust sources too");
    }

    #[test]
    fn talk_about_notices_or_other_holders_is_fine() {
        for line in [
            "fails on Adobe copyright notices",
            "Copyright 2024 the VectorCraft authors",
            "(c) 2020 Someone; Adobe-compatible",
            "PostScript Language Reference, Adobe Systems Incorporated",
        ] {
            assert!(scan("docs/x.md", line.as_bytes()).is_empty(), "{line}");
        }
    }

    #[test]
    fn allowed_licence_texts_may_credit_adobe() {
        assert!(scan("NOTICE", b"Copyright 1990-2023 Adobe. All rights reserved.").is_empty());
        assert!(scan("assets/fonts/OFL-SourceSans3.txt", b"Copyright 2010-2024 Adobe (http://www.adobe.com/)").is_empty());
        assert_eq!(scan("assets/fonts/OFL-Other.txt", b"Copyright 2010 Adobe").len(), 1);
    }

    #[test]
    fn finds_fixtures_an_adobe_application_made() {
        let eps = b"%!PS-Adobe-3.0 EPSF-3.0\n%%Creator: Adobe Illustrator(R) 8.0\n";
        assert!(scan("crates/eps/tests/data/a.eps", eps)[0].contains("%%Creator: Adobe"));
        let svg = b"<!-- Generator: Adobe Illustrator 27.0.0, SVG Export Plug-In -->\n<svg/>";
        assert_eq!(scan("tests/a.svg", svg).len(), 1);
        let pdf = b"%PDF-1.4\n\x00\x01 /Producer (Adobe PDF Library 15.0) /Creator (Adobe InDesign)";
        assert!(scan("tests/a.pdf", pdf)[0].contains("Adobe PDF Library"));
        let xmp = b"<x:xmpmeta><rdf:Description xmp:CreatorTool=\"Adobe Photoshop 25.0\"/></x:xmpmeta>";
        assert!(scan("tests/a.xmp", xmp)[0].contains("xmp:CreatorTool"));
        let element = b"<xmp:CreatorTool>Adobe Illustrator 28.0</xmp:CreatorTool>";
        assert_eq!(scan("tests/b.xmp", element).len(), 1);
        let tiff = b"II*\x00\x08\x00\x00\x00 Adobe Photoshop 2024 (Macintosh)\x00";
        assert!(scan("tests/a.tif", tiff)[0].contains("Adobe Photoshop"));
    }

    #[test]
    fn our_own_files_and_rust_sources_naming_headers_pass() {
        assert!(
            scan("tests/a.eps", b"%!PS-Adobe-3.0 EPSF-3.0\n%%Creator: VectorCraft 0.10\n").is_empty(),
            "the PS-Adobe DSC header is a format name"
        );
        assert!(scan("tests/a.svg", b"<svg xmlns:xmp=\"x\"><metadata>xmp:CreatorTool=\"VectorCraft\"</metadata></svg>").is_empty());
        assert!(scan("crates/eps/src/import/tests_illustrator.rs", b"const AI8: &str = \"%%Creator: Adobe Illustrator(R) 8.0\\n\";").is_empty());
        assert!(scan("docs/x.md", b"Illustrator files say %%Creator: Adobe Illustrator").len() == 1, "outside Rust, the marker counts");
        assert!(
            scan("assets/fonts/Font.ttf", b"\x00\x01\x00\x00 Copyright 2010 Adobe \x00").is_empty(),
            "a binary's copyright string isn't a pasted notice"
        );
    }
}
