---
name: update-cursors
description: Sync VectorCraft's tool cursors with a cursorcraft folder (SVG designs → generated glyphs). Use when the user runs /update-cursors, says cursors were handed over from cursorcraft, or asks to update/wire cursor designs. Also writes the cursors VectorCraft still needs back into the designer's 00-INFO.md.
---

# /update-cursors [folder] [group]

Brings cursorcraft's designs into VectorCraft and tells the designer what VectorCraft still needs.
Design: `plan/cursor-sync-design.md`. Every decision is yours; the record test keeps you honest.

## Rules
- Clean-room: never open cursorcraft's `working/` folder. Needed sections describe VectorCraft's
  behaviour (tool, state, modifier keys, hotspot), citing public-doc notes in `plan/illustrator/`,
  never what Adobe's cursors look like.
- Never hand-edit a generated `const`: regenerate it. Only edit `GENERATED` and the consts block
  in `crates/ui-egui/src/cursors/glyphs.rs`, and `cursors.toml` beside it.
- In `00-INFO.md`, only append sections, or write back wording the designer approved.
- Never invent a tool to give a cursor a home: record it `skipped` and list it.

## 1. Orient
- cursorcraft = the argument's checkout, else `$CURSORCRAFT_DIR`, else `~/projects/cursorcraft`;
  folder = argument, else `<cursorcraft>/svg-vectorcraft`. Missing → stop and say where you looked.
- `cargo run -q --manifest-path <cursorcraft>/Cargo.toml -- check <folder>`: problems → stop and
  list them for the designer. To-dos are fine.
- Source id: `git -C <cursorcraft> rev-parse --short HEAD`, plus `-dirty` if
  `git -C <folder> status --porcelain -- .` isn't empty (warn the designer).

## 2. Diff
- `… -- status <folder>` → JSON rows `{file, const, svg_fp, info_fp, needed, drawn, error}`.
- Compare with `crates/ui-egui/src/cursors/cursors.toml` by `svg`:
  new (no entry) · redrawn (svg_fp differs) · re-described (info_fp differs) · needed-now-drawn
  (entry `needed`, row `drawn`) · removed (entry, no row) · rename (removed + new with the same
  svg_fp: keep the decision) · unchanged.
- A first sync or one with more than ~15 cursors to decide: work in groups (the `group` argument,
  else ask). Order: pen family + eyedropper + knife + mesh; selection and direct selection; type;
  transform, resize, rotate and zoom; the rest. One review and one commit per group.

## 3. Interpret
- Unchanged: nothing. Redrawn only: regenerate, keep the decision.
- Otherwise read the row's `00-INFO.md` section, the `Cursor` enum (`crates/tools/src/lib.rs`),
  the tools' `fn cursor(` (`rg -n "fn cursor\(" crates/tools/src`), `plan/illustrator/` notes,
  and the old entry's `note`. Propose per cursor one of:
  - existing slot(s) → `GENERATED` entries;
  - a new `Cursor` variant + the tool change that returns it (doc comment on the variant; add it
    to `Cursor::precise()` if it's a drawing tool's pointer; `cursor_icon` in
    `crates/ui-egui/src/canvas.rs` must cover it; add it to `GLYPHS` in `cursors.rs` tests;
    a tool test for the state);
  - skip (VectorCraft has no such tool or state).
- Removed: propose falling back to the hand-drawn glyph (drop it from `GENERATED`, entry removed),
  or dropping a variant nothing else draws.
- A slot that stayed a system cursor (Hand, HandGrab, ZoomIn, ZoomOut, NotAllowed) becomes a
  bitmap once wired: update `every_glyph_becomes_a_bitmap_with_its_hotspot_inside`'s system-cursor
  list and say so in the review.
- Only the marker line removed (info_fp unchanged, the row no longer `needed`): no decision.

## 4. Converse
- Batch every uncertain call into one round of questions (AskUserQuestion, ≤4 per call, more
  rounds if needed): name the cursor, what you'd do, and the alternative.
- When an answer clarifies the description, show the new wording; on approval, rewrite that
  section in `00-INFO.md` (keep the marker line if present).

## 5. Find needs
- For every tool (`crates/tools/src/catalog.rs`) and state its `cursor()` returns, find slots with
  no wired or needed entry, notably tools on the generic `Cursor::Crosshair`.
- For each, append to `00-INFO.md`:

  ```
  ## <name>.svg

  Needed by VectorCraft (`Cursor::<Variant>`).

  <Tool>, <state / modifier keys / what's under the pointer>: <what the cursor must convey>.
  Hotspot: <where the click lands>. Source: <helpx URL from plan/illustrator notes, if any>.
  ```
  and record it `status = "needed"`, `slots = [<Variant>]`. The variant must exist: if the need
  is a state VectorCraft doesn't distinguish yet, add the variant (with its tool change, test,
  `precise()` and `cursor_icon` entries) in this sync, or leave the need out.
- A `needed` entry VectorCraft no longer needs: set `status = "obsolete"`, `slots = []`, and ask
  the designer to delete the section.

## 6. Apply
1. Before images: `CURSOR_SHEET_DIR=<scratch>/before cargo test -q -p vectorcraft-ui-egui --lib write_cursor_pngs -- --ignored`.
2. `… -- gen <folder>/<file>.svg …` for every wired cursor of this group; replace those consts
   in `glyphs.rs` (whole const including its doc comment); update `GENERATED` (sorted like the
   `Cursor` enum). Remove the `#[expect(dead_code …)]` lines the first time.
3. Write `cursors.toml`: `cursorcraft = "<source id>"`; one `[[cursor]]` per row of this group,
   fields `svg, svg_fp, info_fp, status, slots, note` (needed: no `svg_fp`), sorted by `svg`.
4. Tool / variant changes with their tests.
5. `CARGO_TARGET_DIR=$PWD/target cargo xtask ci` until green.
6. Docs touched by the change: `docs/ui-parity.md` cursor rows, `docs/target-app-parity.md`
   evidence, their status lines and revision history.

## 7. Review and commit
- After images: `CURSOR_SHEET_DIR=<scratch>/after …write_cursor_pngs…`, then
  `SHEET_BEFORE=<scratch>/before SHEET_AFTER=<scratch>/after SHEET_OUT=plan/cursors/sync-<group>.png cargo test -q -p vectorcraft-ui-egui --lib compose_review_sheet -- --ignored`.
  Read the PNG yourself first; then give the designer its path, its row list (`.txt`; columns are
  before 1×, after 1×, before 2×, after 2× on light, grey, dark), the decisions, skips and needs
  added.
- On the designer's OK: `git add` the touched files only, commit to `main`:
  `cursors: sync from cursorcraft <source id> (<group>)`, body listing wired / skipped / needed.
- Tell the designer `00-INFO.md` changed in cursorcraft (theirs to commit), and to run
  `cursorcraft check` to see the new to-dos.
