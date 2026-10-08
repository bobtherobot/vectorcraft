//! Drags redraw only what moves. While a canvas drag previews (moving or transforming objects,
//! dragging anchors and handles, drawing shapes and paths, reshaping and distorting), the art below
//! and above the art that changes is rendered once and kept as two textures; each frame renders
//! just the changing art, in its bounds, between them. Rendering the whole canvas every frame made the art trail the
//! pointer, and on heavy documents fall back to the background worker and lag its own outline.
//!
//! The split is exact only when the moving art doesn't interact with the rest, so [`plan`] uses it
//! only then and the canvas otherwise renders whole frames as before:
//! - the preview is one of [`SPLIT_COMMANDS`], which change nothing but the objects they act on,
//!   and nothing document-wide that drawing reads changed ([`same_resources`]);
//! - documents are copy-on-write: every object the drag didn't touch is the same `Arc` as before
//!   it, so the art that changed is found exactly, and nothing else changed;
//! - the changed art is contiguous in stacking order, under containers that paint nothing of their
//!   own (no opacity, blending, masks, clipping or appearance: their contents composite one by one);
//! - neither it nor the art above it blends with what is below (blend modes, knockout), and it
//!   doesn't wrap or thread type that other objects lay out around or into.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use egui::{Color32, Painter, Rect, pos2};
use vectorcraft_color::BlendMode;
use vectorcraft_doc::{Document, Knockout, Node, NodeId, NodeKind};
use vectorcraft_geom::kurbo::Affine;
use vectorcraft_render::{RenderOptions, Rendered};

use crate::{CacheKey, VectorcraftApp};

/// Preview commands that change only the objects they act on (or create). Previews that change
/// artboards, slices, the print tiling or the perspective grid aren't here.
const SPLIT_COMMANDS: &[&str] = &[
    // Selection, Direct Selection, Free Transform, live corners, anchor tools.
    "object.transform",
    "object.distort",
    "object.setLiveShape",
    "path.moveAnchors",
    "path.setHandle",
    "path.convertAnchor",
    "path.reshape",
    "path.reshapeSegment",
    // Shapes and grids.
    "shape.rectangle",
    "shape.ellipse",
    "shape.polygon",
    "shape.star",
    "shape.line",
    "shape.arc",
    "shape.spiral",
    "shape.rectangularGrid",
    "shape.polarGrid",
    "shape.flare",
    // Pen, Curvature, Pencil, Paintbrush.
    "path.create",
    "path.appendAnchor",
    "path.close",
    "path.curvature",
    "path.freehand",
    "brush.freehand",
    // Cutting.
    "path.lineCut",
    "path.rectCut",
    "path.mirrorCut",
    // Width, mesh, blends, liquify, puppet warp, freeform gradients.
    "stroke.widthPoint.set",
    "stroke.widthPoint.copy",
    "stroke.widthProfile.set",
    "object.mesh.movePoint",
    "object.blend.spine.moveAnchor",
    "object.liquify",
    "object.puppetWarp",
    "paint.freeform.setPoint",
    "paint.freeform.addPoint",
    "paint.freeform.deletePoint",
    "paint.freeform.splitLine",
    // Graphs, symbolism tools, typing.
    "graph.create",
    "symbol.spray",
    "symbol.adjust",
    "text.editRange",
];

/// Pixels of margin around the moving art's bounds (antialiasing).
const MARGIN_PX: f64 = 2.0;

/// How a drag's art splits: the topmost unchanged subtrees below the moving art, the moving art,
/// and the unchanged subtrees above it, each in stacking order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub below: Vec<NodeId>,
    pub moving: Vec<NodeId>,
    pub above: Vec<NodeId>,
}

impl Plan {
    /// What the render of the art below hides.
    fn hide_for_below(&self) -> Vec<NodeId> {
        self.moving.iter().chain(&self.above).copied().collect()
    }
    /// What the render of the art above hides.
    fn hide_for_above(&self) -> Vec<NodeId> {
        self.moving.iter().chain(&self.below).copied().collect()
    }
    /// What the render of the moving art hides.
    fn hide_for_moving(&self) -> Vec<NodeId> {
        self.below.iter().chain(&self.above).copied().collect()
    }
}

/// The pixels (x0, y0, width, height) of a `w`×`h` canvas seen through `view` that the moving art
/// of `plan` can touch, or `None` when it shows nowhere.
fn moving_area(doc: &Document, plan: &Plan, view: Affine, w: u32, h: u32) -> Option<(f64, f64, u32, u32)> {
    let bounds = plan.moving.iter().filter_map(|id| doc.node(*id)?.visual_bounds()).reduce(|a, b| a.union(b))?;
    let canvas = vectorcraft_geom::Rect::new(0.0, 0.0, f64::from(w), f64::from(h));
    let a = view.transform_rect_bbox(bounds).inflate(MARGIN_PX, MARGIN_PX).intersect(canvas);
    if !a.is_finite() || a.width() < 1.0 || a.height() < 1.0 {
        return None;
    }
    let (x0, y0) = (a.x0.floor(), a.y0.floor());
    Some((x0, y0, (a.x1.ceil() - x0) as u32, (a.y1.ceil() - y0) as u32))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Below,
    Moving,
    Above,
}

/// The split of `cur`, the document a `cmd` preview made from `base`, or `None` when drawing it
/// in parts would not look exactly like drawing it whole.
pub fn plan(base: &Document, cur: &Document, cmd: &str) -> Option<Plan> {
    if !SPLIT_COMMANDS.contains(&cmd) || cur.page_isolate || cur.page_knockout || cur.pattern_edit.is_some() || cur.mask_edit.is_some() {
        return None;
    }
    if !same_resources(base, cur) {
        return None;
    }
    let mut plan = Plan::default();
    let mut phase = Phase::Below;
    split(&base.layers, &cur.layers, &mut plan, &mut phase)?;
    if plan.moving.is_empty() {
        return None;
    }
    let threaded: HashSet<NodeId> = cur.text_threads.iter().flatten().copied().collect();
    for id in &plan.moving {
        let n = cur.node(*id)?;
        if !keeps_to_itself(n) || subtree_any(n, &mut |c| threaded.contains(&c.id) || c.wrap.is_some()) {
            return None;
        }
    }
    Some(plan)
}

/// Whether everything document-wide that drawing an object reads is the same in `base` and `cur`:
/// colours and swatches, styles, symbols, patterns, images, brushes, threads, colour settings.
/// (Unchanged objects drawn from `cur` must look as they did.)
fn same_resources(base: &Document, cur: &Document) -> bool {
    let symbols = base.symbols.len() == cur.symbols.len()
        && base.symbols.iter().zip(&cur.symbols).all(|(a, b)| a.name == b.name && Arc::ptr_eq(&a.art, &b.art));
    symbols
        && base.color_mode == cur.color_mode
        && base.spot_use_lab == cur.spot_use_lab
        && base.swatches == cur.swatches
        && base.swatch_groups == cur.swatch_groups
        && base.graphic_styles == cur.graphic_styles
        && base.char_styles == cur.char_styles
        && base.para_styles == cur.para_styles
        && base.text_threads == cur.text_threads
        && base.patterns == cur.patterns
        && base.images == cur.images
        && base.setup == cur.setup
        && base.raster_effects_ppi == cur.raster_effects_ppi
        && base.raster_effects == cur.raster_effects
        && base.color_profiles == cur.color_profiles
        && base.unknown.get("brushes") == cur.unknown.get("brushes")
}

/// Sorts `cur` (children of a container, bottom first) against the same container's `base`.
fn split(base: &[Arc<Node>], cur: &[Arc<Node>], plan: &mut Plan, phase: &mut Phase) -> Option<()> {
    // Usually nothing was added, removed or reordered: match by position, else by id.
    let same_order = base.len() == cur.len() && base.iter().zip(cur).all(|(b, c)| b.id == c.id);
    let by_id: HashMap<NodeId, &Arc<Node>> = if same_order { HashMap::new() } else { base.iter().map(|n| (n.id, n)).collect() };
    for (i, c) in cur.iter().enumerate() {
        let b = if same_order { base.get(i) } else { by_id.get(&c.id).copied() };
        match b {
            Some(b) if Arc::ptr_eq(b, c) => {
                if *phase == Phase::Below {
                    plan.below.push(c.id);
                } else {
                    *phase = Phase::Above;
                    plan.above.push(c.id);
                }
            }
            Some(b) if paints_nothing(b) && paints_nothing(c) => split(b.children()?, c.children()?, plan, phase)?,
            _ => {
                // Changed or new: moving art, all of it in one run.
                if *phase == Phase::Above {
                    return None;
                }
                *phase = Phase::Moving;
                plan.moving.push(c.id);
            }
        }
    }
    Some(())
}

/// A layer or plain group: its members composite one by one, as if it weren't there.
fn paints_nothing(n: &Node) -> bool {
    let plain_kind = matches!(n.kind, NodeKind::Layer { template: false, clip: false, .. } | NodeKind::Group { clip: false, .. });
    plain_kind
        && n.opacity == 1.0
        && n.blend == BlendMode::Normal
        && n.knockout == Knockout::Neutral
        && n.mask.is_none()
        && n.appearance.items.is_empty()
        && n.appearance.effects.is_empty()
}

/// Whether `n`'s art looks the same drawn alone and laid over what's below it: nothing in it
/// blends with or knocks out its backdrop.
fn keeps_to_itself(n: &Node) -> bool {
    !subtree_any(n, &mut |c| c.blend != BlendMode::Normal || c.knockout != Knockout::Neutral)
}

fn subtree_any(n: &Node, test: &mut dyn FnMut(&Node) -> bool) -> bool {
    let mut any = false;
    n.walk(&mut |c| any |= test(c));
    any
}

/// What the cached layers were drawn for.
#[derive(Clone, Debug, PartialEq)]
struct LayersKey {
    view: CacheKey,
    /// The document the drag started from.
    base: usize,
    plan: Plan,
}

/// The art below and above a drag's moving objects, and the moving art as last drawn.
pub struct DragLayers {
    key: LayersKey,
    /// The art above can't be drawn apart (it blends): the canvas renders whole frames.
    whole: bool,
    below: Option<egui::TextureHandle>,
    above: Option<egui::TextureHandle>,
    moving: Option<(egui::TextureHandle, Rect)>,
    /// What the moving art was hidden from, for each frame's render.
    others: Vec<NodeId>,
    /// The document's layers as last drawn: after the drag the layers stand in for the canvas
    /// until its own render of the same art arrives.
    drawn: Vec<Arc<Node>>,
}

/// Draw the art of a drag in progress on `painter` as below / moving / above layers, when the
/// drag allows it. `key` is the frame's whole-canvas render key, `view` its document → pixel
/// transform. Returns false (drawing nothing) to have the canvas render the whole frame.
pub fn show(
    app: &mut VectorcraftApp,
    ctx: &egui::Context,
    painter: &Painter,
    rect: Rect,
    key: &CacheKey,
    view: Affine,
    opts: &RenderOptions,
) -> bool {
    let Some(st) = app.session.active() else { return false };
    let Some(it) = &st.interaction else { return false };
    let Some((cmd, _)) = &it.preview else { return false };
    let Some(plan) = plan(&it.doc, &st.doc, cmd) else { return false };
    let doc = st.doc.clone();
    let lkey = LayersKey { view: CacheKey { revision: 0, ..key.clone() }, base: Arc::as_ptr(&it.doc) as usize, plan };
    if app.canvas.drag.as_ref().is_none_or(|d| d.key != lkey) {
        app.canvas.drag = Some(build(app, ctx, &doc, lkey, key, view, opts));
    }
    let Some(layers) = app.canvas.drag.as_mut() else { return false };
    if layers.whole {
        return false;
    }
    // The moving art, in its bounds only.
    layers.moving = match moving_area(&doc, &layers.key.plan, view, key.w, key.h) {
        Some((x0, y0, w, h)) => {
            let opts = RenderOptions { hidden: layers.others.clone(), ..opts.clone() };
            let img = app.canvas.renderer.render(&doc, w, h, Affine::translate((-x0, -y0)) * view, &opts);
            let ppp = f64::from(key.ppp.max(1e-3));
            let at = Rect::from_min_size(
                rect.min + egui::vec2((x0 / ppp) as f32, (y0 / ppp) as f32),
                egui::vec2((f64::from(w) / ppp) as f32, (f64::from(h) / ppp) as f32),
            );
            let tex = match layers.moving.take() {
                Some((mut tex, _)) => {
                    tex.set(color_image(&img), egui::TextureOptions::LINEAR);
                    tex
                }
                None => ctx.load_texture("canvas-drag-moving", color_image(&img), egui::TextureOptions::LINEAR),
            };
            Some((tex, at))
        }
        None => None,
    };
    layers.drawn = doc.layers.clone();
    paint(layers, painter, rect);
    true
}

/// After a drag: draw its last layers while the canvas's own render of the same art (`doc`, seen
/// as `key`) is pending, so the art doesn't jump back to where the drag started. Returns false
/// when they show something else.
pub fn show_last(app: &VectorcraftApp, painter: &Painter, rect: Rect, doc: &Document, key: &CacheKey) -> bool {
    let Some(layers) = &app.canvas.drag else { return false };
    let same_art = layers.drawn.len() == doc.layers.len() && layers.drawn.iter().zip(&doc.layers).all(|(a, b)| Arc::ptr_eq(a, b));
    if layers.whole || !same_art || layers.key.view != (CacheKey { revision: 0, ..key.clone() }) {
        return false;
    }
    paint(layers, painter, rect);
    true
}

fn paint(layers: &DragLayers, painter: &Painter, rect: Rect) {
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    if let Some(tex) = &layers.below {
        painter.image(tex.id(), rect, uv, Color32::WHITE);
    }
    if let Some((tex, at)) = &layers.moving {
        painter.image(tex.id(), *at, uv, Color32::WHITE);
    }
    if let Some(tex) = &layers.above {
        painter.image(tex.id(), rect, uv, Color32::WHITE);
    }
}

/// Render the art below and above the moving art for a new drag (or view).
fn build(
    app: &mut VectorcraftApp,
    ctx: &egui::Context,
    doc: &Document,
    lkey: LayersKey,
    key: &CacheKey,
    view: Affine,
    opts: &RenderOptions,
) -> DragLayers {
    let plan = &lkey.plan;
    let whole = !plan.above.iter().filter_map(|id| doc.node(*id)).all(keeps_to_itself);
    let mut layers = DragLayers { key: lkey.clone(), whole, below: None, above: None, moving: None, others: vec![], drawn: vec![] };
    if whole {
        return layers;
    }
    let mut render = |hidden: Vec<NodeId>, name: &str| {
        let img = app.canvas.renderer.render(doc, key.w, key.h, view, &RenderOptions { hidden, ..opts.clone() });
        ctx.load_texture(name, color_image(&img), egui::TextureOptions::LINEAR)
    };
    layers.below = (!plan.below.is_empty()).then(|| render(plan.hide_for_below(), "canvas-drag-below"));
    layers.above = (!plan.above.is_empty()).then(|| render(plan.hide_for_above(), "canvas-drag-above"));
    layers.others = plan.hide_for_moving();
    layers
}

fn color_image(img: &Rendered) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied([img.width as usize, img.height as usize], &img.pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vectorcraft_engine::Session;
    use vectorcraft_render::Renderer;

    /// A session with three rectangles stacked bottom to top → (session, [bottom, middle, top]).
    fn three() -> (Session, [NodeId; 3]) {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 300, "height": 300})).unwrap();
        let ids = [0.0, 50.0, 100.0].map(|x| {
            let id = s.execute("shape.rectangle", &json!({"x": x, "y": 20, "width": 80, "height": 80})).unwrap()["id"].as_u64().unwrap();
            NodeId(id)
        });
        (s, ids)
    }

    /// Preview moving `ids` 10 pt right with `cmd` → (document before, document after).
    fn drag(s: &mut Session, ids: &[NodeId], cmd: &str) -> (Arc<Document>, Arc<Document>) {
        let ids: Vec<u64> = ids.iter().map(|i| i.0).collect();
        s.execute("select.set", &json!({"ids": ids})).unwrap();
        s.begin_interaction("Move").unwrap();
        let base = s.active().unwrap().doc.clone();
        let params = if cmd == "object.transform" { json!({"matrix": [1, 0, 0, 1, 10, 0], "ids": ids}) } else { json!({"dx": 10, "dy": 0}) };
        s.preview(cmd, &params).unwrap();
        (base, s.active().unwrap().doc.clone())
    }

    #[test]
    fn a_moved_object_splits_the_art_below_and_above_it() {
        let (mut s, [a, b, c]) = three();
        let (base, cur) = drag(&mut s, &[b], "object.transform");
        assert_eq!(plan(&base, &cur, "object.transform"), Some(Plan { below: vec![a], moving: vec![b], above: vec![c] }));
        // Other previews may change anything: no split.
        assert_eq!(plan(&base, &cur, "object.expand"), None);
        s.cancel_interaction().unwrap();
        let (base, cur) = drag(&mut s, &[c], "path.moveAnchors");
        assert_eq!(plan(&base, &cur, "path.moveAnchors"), Some(Plan { below: vec![a, b], moving: vec![c], above: vec![] }));
    }

    #[test]
    fn moving_art_must_be_one_run_and_keep_to_itself() {
        let (mut s, [a, b, c]) = three();
        let (base, cur) = drag(&mut s, &[a, c], "object.transform");
        assert_eq!(plan(&base, &cur, "object.transform"), None, "unchanged art between moving art");
        s.cancel_interaction().unwrap();
        s.execute("transparency.set", &json!({"ids": [b.0], "blend": "multiply"})).unwrap();
        let (base, cur) = drag(&mut s, &[b], "object.transform");
        assert_eq!(plan(&base, &cur, "object.transform"), None, "it blends with the art below");
    }

    /// Premultiplied `top` over `dst`, `top` placed at (x0, y0).
    fn over(dst: &mut Rendered, top: &Rendered, x0: u32, y0: u32) {
        for y in 0..top.height {
            for x in 0..top.width {
                let (s, d) = (((y * top.width + x) * 4) as usize, (((y + y0) * dst.width + x + x0) * 4) as usize);
                let a = u32::from(top.pixels[s + 3]);
                for k in 0..4 {
                    let v = u32::from(top.pixels[s + k]) + (u32::from(dst.pixels[d + k]) * (255 - a) + 127) / 255;
                    dst.pixels[d + k] = v.min(255) as u8;
                }
            }
        }
    }

    /// The layers laid over each other look like the whole art rendered at once.
    #[test]
    fn the_layers_composite_to_the_whole_render() {
        let (mut s, [a, b, c]) = three();
        // Paints that show any misordering, the moving one translucent.
        for (id, hex) in [(a, "#ff0000"), (b, "#00ff00"), (c, "#0000ff")] {
            s.execute("paint.setFill", &json!({"ids": [id.0], "color": hex})).unwrap();
        }
        s.execute("transparency.set", &json!({"ids": [b.0], "opacity": 60})).unwrap();
        let (base, cur) = drag(&mut s, &[b], "object.transform");
        let p = plan(&base, &cur, "object.transform").unwrap();
        let whole = Renderer::new().render(&cur, 300, 200, VIEW, &RenderOptions::default());
        assert!(whole.pixels.as_chunks::<4>().0.iter().any(|px| px[1] > 100 && px[0] < 60), "the moving (green) art shows");
        let worst = layers_vs_whole(&cur, &p);
        assert!(worst <= 2, "layers differ from the whole render by up to {worst}");
    }

    const VIEW: Affine = Affine::new([1.37, 0.0, 0.0, 1.37, 4.52, 2.33]);

    /// How far (in 8-bit levels) `plan`'s layers of `doc`, laid over each other, are from `doc`
    /// rendered whole, on a 300×200 canvas.
    fn layers_vs_whole(doc: &Document, p: &Plan) -> u8 {
        let (w, h) = (300, 200);
        let mut r = Renderer::new();
        let mut render = |hidden: Vec<NodeId>, w, h, view| r.render(doc, w, h, view, &RenderOptions { hidden, ..Default::default() });
        let whole = render(vec![], w, h, VIEW);
        let mut layered = render(p.hide_for_below(), w, h, VIEW);
        if let Some((x0, y0, mw, mh)) = moving_area(doc, p, VIEW, w, h) {
            let moving = render(p.hide_for_moving(), mw, mh, Affine::translate((-x0, -y0)) * VIEW);
            over(&mut layered, &moving, x0 as u32, y0 as u32);
        }
        over(&mut layered, &render(p.hide_for_above(), w, h, VIEW), 0, 0);
        whole.pixels.iter().zip(&layered.pixels).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0)
    }

    /// Every command listed exists.
    #[test]
    fn split_commands_are_commands() {
        for c in SPLIT_COMMANDS {
            assert!(vectorcraft_engine::find_command(c).is_some(), "{c} is not a command");
        }
    }

    /// Each drawing tool, dragged over overlapping art (the middle square selected): its preview
    /// is drawn in layers, and they look like the whole render (but art that blends).
    #[test]
    fn drawing_tools_drag_in_layers_that_match_the_whole_render() {
        use vectorcraft_engine::ViewInfo;
        use vectorcraft_tools::{PointerEvent, PointerKind};
        const TOOLS: &[&str] = &[
            "selection",
            "directSelection",
            "groupSelection",
            "rectangle",
            "roundedRectangle",
            "ellipse",
            "polygon",
            "star",
            "flare",
            "lineSegment",
            "arc",
            "spiral",
            "rectangularGrid",
            "polarGrid",
            "pencil",
            "paintbrush",
            "pen",
            "scale",
            "rotate",
            "shear",
            "reflect",
            "freeTransform",
            "warp",
            "twirl",
            "lineCut",
            "rectCut",
        ];
        let mut report = vec![];
        for tool in TOOLS {
            let (mut s, [a, b, c]) = three();
            for (id, hex) in [(a, "#ff0000"), (b, "#00ff00"), (c, "#0000ff")] {
                s.execute("paint.setFill", &json!({"ids": [id.0], "color": hex})).unwrap();
            }
            s.execute("select.set", &json!({"ids": [b.0]})).unwrap();
            let view = ViewInfo { smart_guides: false, ..Default::default() };
            s.select_tool(tool, view).unwrap();
            let edits = matches!(
                *tool,
                "selection" | "directSelection" | "groupSelection" | "scale" | "rotate" | "shear" | "reflect" | "freeTransform" | "warp" | "twirl"
            );
            let (from, to) = if edits { ((90.0, 60.0), (140.0, 130.0)) } else { ((20.0, 60.0), (200.0, 160.0)) };
            s.pointer(&PointerEvent::new(PointerKind::Down, from.0, from.1), view).unwrap();
            for k in 1..=6 {
                let t = f64::from(k) / 6.0;
                let ev = PointerEvent::new(PointerKind::Drag, from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
                s.pointer(&ev, view).unwrap();
            }
            let st = s.active().unwrap();
            let Some(it) = &st.interaction else {
                report.push(format!("{tool}: no preview"));
                continue;
            };
            let cmd = it.preview.as_ref().map_or("", |p| p.0.as_str());
            match plan(&it.doc, &st.doc, cmd) {
                Some(p) => {
                    let worst = layers_vs_whole(&st.doc, &p);
                    if worst > 2 {
                        report.push(format!("{tool} ({cmd}): layers off by {worst}"));
                    }
                }
                None => report.push(format!("{tool} ({cmd}): drawn whole")),
            }
        }
        // The flare is drawn in Screen mode, blending with the art below: it is drawn whole.
        assert_eq!(report, vec!["flare (shape.flare): drawn whole".to_string()]);
    }

    /// Drawing a shape: the new shape is the changing art, on top, the same from frame to frame
    /// (so the art below is rendered once).
    #[test]
    fn a_shape_being_drawn_is_the_changing_art() {
        let (mut s, ids) = three();
        s.begin_interaction("Rectangle").unwrap();
        let base = s.active().unwrap().doc.clone();
        let mut plans = vec![];
        for w in [20, 40] {
            s.preview("shape.rectangle", &json!({"x": 10, "y": 150, "width": w, "height": 30})).unwrap();
            plans.push(plan(&base, &s.active().unwrap().doc, "shape.rectangle").unwrap());
        }
        assert_eq!(plans[0], plans[1]);
        assert_eq!((plans[0].below.clone(), plans[0].moving.len(), plans[0].above.len()), (ids.to_vec(), 1, 0));
    }

    /// A preview that changes something document-wide (here a swatch other art may use) is drawn
    /// whole.
    #[test]
    fn a_document_wide_change_is_drawn_whole() {
        let (mut s, [_, b, _]) = three();
        let (base, cur) = drag(&mut s, &[b], "object.transform");
        let mut changed = (*cur).clone();
        changed.swatches.pop();
        assert!(plan(&base, &cur, "object.transform").is_some());
        assert_eq!(plan(&base, &changed, "object.transform"), None);
    }

    #[test]
    fn a_group_with_its_own_opacity_moves_as_one() {
        let (mut s, [a, b, c]) = three();
        s.execute("select.set", &json!({"ids": [b.0, c.0]})).unwrap();
        let g = NodeId(s.execute("object.group", &json!({})).unwrap()["id"].as_u64().unwrap());
        s.execute("transparency.set", &json!({"ids": [g.0], "opacity": 50})).unwrap();
        // Moving one member: the translucent group is drawn whole, never split inside.
        let (base, cur) = drag(&mut s, &[b], "object.transform");
        assert_eq!(plan(&base, &cur, "object.transform"), Some(Plan { below: vec![a], moving: vec![g], above: vec![] }));
    }
}
