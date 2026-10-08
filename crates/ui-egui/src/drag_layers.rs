//! Drags redraw only what moves. While a canvas drag previews (moving or transforming objects with
//! the Selection tool, dragging anchors with Direct Selection), the art below and above the moving
//! objects is rendered once and kept as two textures; each frame renders just the moving objects,
//! in their bounds, between them. Rendering the whole canvas every frame made the art trail the
//! pointer, and on heavy documents fall back to the background worker and lag its own outline.
//!
//! The split is exact only when the moving art doesn't interact with the rest, so [`plan`] uses it
//! only then and the canvas otherwise renders whole frames as before:
//! - the preview is one of [`SPLIT_COMMANDS`], which change nothing but the objects they move;
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

/// Preview commands that change only the objects they act on.
const SPLIT_COMMANDS: [&str; 2] = ["object.transform", "path.moveAnchors"];

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
        let (w, h) = (300, 200);
        let view = Affine::scale(1.37) * Affine::translate((3.3, 1.7));
        let mut r = vectorcraft_render::Renderer::new();
        let render =
            |r: &mut vectorcraft_render::Renderer, hidden: Vec<NodeId>| r.render(&cur, w, h, view, &RenderOptions { hidden, ..Default::default() });
        let whole = render(&mut r, vec![]);
        assert!(whole.pixels.as_chunks::<4>().0.iter().any(|px| px[1] > 100 && px[0] < 60), "the moving (green) art shows");
        let mut layered = render(&mut r, p.hide_for_below());
        let (x0, y0, mw, mh) = moving_area(&cur, &p, view, w, h).unwrap();
        let moving =
            r.render(&cur, mw, mh, Affine::translate((-x0, -y0)) * view, &RenderOptions { hidden: p.hide_for_moving(), ..Default::default() });
        over(&mut layered, &moving, x0 as u32, y0 as u32);
        over(&mut layered, &render(&mut r, p.hide_for_above()), 0, 0);
        let worst = whole.pixels.iter().zip(&layered.pixels).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
        assert!(worst <= 2, "layers differ from the whole render by up to {worst}");
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
