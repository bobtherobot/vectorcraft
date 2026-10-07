//! Tool cursors drawn as vector glyphs. Black shapes with a white halo, hotspot at `p`,
//! Illustrator's visual grammar: solid arrow (Selection), hollow arrow (Direct Selection), pen nib
//! with state badges, crosshair for drawing tools, curved arrows for rotate.
//!
//! On the desktop each glyph becomes an OS cursor bitmap ([`os_image`]), which the system moves
//! at hardware speed; one painted into the frame ([`paint`], the web's only option) trails the
//! mouse by the frames in flight.

use egui::epaint::{Mesh, TessellationOptions, Tessellator};
use egui::{Color32, CustomCursorImage, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use std::sync::Arc;
use vectorcraft_tools::Cursor;

const INK: Color32 = Color32::BLACK;
const HALO: Color32 = Color32::WHITE;

/// The largest OS cursor bitmap side, in pixels: X11, Windows and macOS all take this size, and
/// a glyph at a higher display scale is drawn smaller to fit.
pub(crate) const MAX_BITMAP: u16 = 128;

/// A glyph's shapes, collected through the few [`Painter`] calls the glyphs use.
#[derive(Default)]
struct Glyph(Vec<Shape>);

impl Glyph {
    fn add(&mut self, shape: Shape) {
        self.0.push(shape);
    }

    fn line_segment(&mut self, pts: [Pos2; 2], stroke: Stroke) {
        self.add(Shape::line_segment(pts, stroke));
    }

    fn circle_filled(&mut self, center: Pos2, radius: f32, fill: Color32) {
        self.add(Shape::circle_filled(center, radius, fill));
    }

    fn circle_stroke(&mut self, center: Pos2, radius: f32, stroke: Stroke) {
        self.add(Shape::circle_stroke(center, radius, stroke));
    }
}

fn poly(p: &mut Glyph, pts: Vec<Pos2>, fill: Color32, stroke: Color32) {
    // Halo first (thicker white outline), then the glyph.
    p.add(Shape::closed_line(pts.clone(), Stroke::new(3.0, HALO)));
    p.add(Shape::convex_polygon(pts.clone(), fill, Stroke::NONE));
    p.add(Shape::closed_line(pts, Stroke::new(1.0, stroke)));
}

fn line(p: &mut Glyph, a: Pos2, b: Pos2) {
    p.line_segment([a, b], Stroke::new(3.0, HALO));
    p.line_segment([a, b], Stroke::new(1.2, INK));
}

fn arrow_points(o: Pos2) -> Vec<Pos2> {
    // Classic pointer, tip at o.
    [(0.0, 0.0), (0.0, 15.0), (3.8, 11.4), (6.4, 17.0), (8.6, 16.0), (6.1, 10.6), (11.0, 10.6)].iter().map(|(x, y)| o + vec2(*x, *y)).collect()
}

fn arrow(p: &mut Glyph, o: Pos2, hollow: bool) {
    let pts = arrow_points(o);
    // The arrow is concave: draw as a filled mesh of two convex parts, then outline.
    p.add(Shape::closed_line(pts.clone(), Stroke::new(3.0, HALO)));
    let fill = if hollow { HALO } else { INK };
    p.add(Shape::convex_polygon(vec![pts[0], pts[1], pts[2], pts[5], pts[6]], fill, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![pts[2], pts[3], pts[4], pts[5]], fill, Stroke::NONE));
    p.add(Shape::closed_line(pts, Stroke::new(1.0, INK)));
}

fn crosshair(p: &mut Glyph, o: Pos2) {
    for (a, b) in
        [(vec2(-9.0, 0.0), vec2(-2.0, 0.0)), (vec2(2.0, 0.0), vec2(9.0, 0.0)), (vec2(0.0, -9.0), vec2(0.0, -2.0)), (vec2(0.0, 2.0), vec2(0.0, 9.0))]
    {
        line(p, o + a, o + b);
    }
}

fn pen(p: &mut Glyph, o: Pos2, badge: &str) {
    // Nib pointing to the top-left hotspot.
    let pts = vec![o, o + vec2(4.0, 12.0), o + vec2(8.0, 16.0), o + vec2(16.0, 8.0), o + vec2(12.0, 4.0)];
    poly(p, pts, INK, INK);
    p.circle_filled(o + vec2(6.0, 6.0), 1.4, HALO);
    line(p, o + vec2(10.0, 14.0), o + vec2(14.0, 18.0));
    let b = o + vec2(16.0, 14.0);
    match badge {
        "o" => {
            p.circle_stroke(b + vec2(3.0, 3.0), 3.0, Stroke::new(3.0, HALO));
            p.circle_stroke(b + vec2(3.0, 3.0), 3.0, Stroke::new(1.2, INK));
        }
        "+" => {
            line(p, b + vec2(0.0, 3.0), b + vec2(6.0, 3.0));
            line(p, b + vec2(3.0, 0.0), b + vec2(3.0, 6.0));
        }
        "-" => line(p, b + vec2(0.0, 3.0), b + vec2(6.0, 3.0)),
        "/" => line(p, b + vec2(0.0, 6.0), b + vec2(5.0, 0.0)),
        "*" => {
            line(p, b + vec2(0.0, 0.0), b + vec2(6.0, 6.0));
            line(p, b + vec2(6.0, 0.0), b + vec2(0.0, 6.0));
            line(p, b + vec2(3.0, -1.0), b + vec2(3.0, 7.0));
        }
        _ => {}
    }
}

fn double_arrow(p: &mut Glyph, o: Pos2, dir: egui::Vec2) {
    let d = dir.normalized() * 8.0;
    let n = vec2(-d.y, d.x) * 0.45;
    line(p, o - d, o + d);
    for (tip, back) in [(o + d, o + d * 0.45), (o - d, o - d * 0.45)] {
        poly(p, vec![tip, back + n, back - n], INK, INK);
    }
}

fn rotate(p: &mut Glyph, o: Pos2) {
    let pts: Vec<Pos2> = (0..=10)
        .map(|i| {
            let a = std::f32::consts::PI * (0.15 + 0.7 * i as f32 / 10.0);
            o + vec2(a.cos() * 9.0, -a.sin() * 9.0)
        })
        .collect();
    p.add(Shape::line(pts.clone(), Stroke::new(3.0, HALO)));
    p.add(Shape::line(pts.clone(), Stroke::new(1.2, INK)));
    for end in [pts[0], pts[pts.len() - 1]] {
        poly(p, vec![end + vec2(-3.0, -1.0), end + vec2(3.0, -1.0), end + vec2(0.0, 4.0)], INK, INK);
    }
}

/// Live Corners: the hollow arrow with a rounded-corner badge.
fn corner_radius(p: &mut Glyph, o: Pos2) {
    arrow(p, o, true);
    let b = o + vec2(12.0, 13.0);
    let mut pts = vec![b + vec2(0.0, 10.0)];
    pts.extend((0..=8).map(|i| {
        let a = std::f32::consts::PI * (1.0 + 0.5 * i as f32 / 8.0);
        b + vec2(4.0 + 4.0 * a.cos(), 4.0 + 4.0 * a.sin())
    }));
    pts.push(b + vec2(10.0, 0.0));
    p.add(Shape::line(pts.clone(), Stroke::new(3.0, HALO)));
    p.add(Shape::line(pts, Stroke::new(1.2, INK)));
}

/// The gradient annotator's stop cursors: the arrow with a plus (add a stop) or minus (delete it)
/// badge.
fn stop_badge(p: &mut Glyph, o: Pos2, add: bool) {
    arrow(p, o, false);
    let b = o + vec2(13.0, 13.0);
    line(p, b, b + vec2(6.0, 0.0));
    if add {
        line(p, b + vec2(3.0, -3.0), b + vec2(3.0, 3.0));
    }
}

/// A slice badge: a small rectangle cut by a line, at `b` (its top left).
fn slice_badge(p: &mut Glyph, b: Pos2) {
    let pts = vec![b, b + vec2(8.0, 0.0), b + vec2(8.0, 6.0), b + vec2(0.0, 6.0)];
    poly(p, pts, HALO, INK);
    line(p, b + vec2(4.0, 0.0), b + vec2(4.0, 6.0));
}

/// The Slice tool: a crosshair with a blade below right of the hotspot.
fn slice(p: &mut Glyph, o: Pos2) {
    crosshair(p, o);
    let b = o + vec2(7.0, 7.0);
    poly(p, vec![b, b + vec2(9.0, 4.0), b + vec2(10.0, 7.0), b + vec2(3.0, 6.0)], HALO, INK);
    line(p, b + vec2(8.0, 6.0), b + vec2(12.0, 12.0));
}

/// The Width tool: the hollow arrow with a stroke that swells in the middle (a width point),
/// plus a badge: `+` over a stroke (a drag adds a point), a bar across the swell over a width point
/// (a drag moves or widens it).
fn width(p: &mut Glyph, o: Pos2, badge: &str) {
    arrow(p, o, true);
    let b = o + vec2(11.0, 18.0);
    let top: Vec<Pos2> = (0..=8)
        .map(|i| {
            let t = i as f32 / 8.0;
            b + vec2(12.0 * t, -3.5 * (std::f32::consts::PI * t).sin())
        })
        .collect();
    let mut lens = top.clone();
    lens.extend(top.iter().rev().map(|q| pos2(q.x, 2.0 * b.y - q.y)));
    p.add(Shape::closed_line(lens.clone(), Stroke::new(3.0, HALO)));
    p.add(Shape::closed_line(lens, Stroke::new(1.2, INK)));
    match badge {
        "+" => {
            let c = b + vec2(16.0, -6.0);
            line(p, c - vec2(3.0, 0.0), c + vec2(3.0, 0.0));
            line(p, c - vec2(0.0, 3.0), c + vec2(0.0, 3.0));
        }
        "point" => line(p, b + vec2(6.0, -6.0), b + vec2(6.0, 6.0)),
        _ => {}
    }
}

fn ibeam(p: &mut Glyph, o: Pos2) {
    line(p, o + vec2(0.0, -8.0), o + vec2(0.0, 8.0));
    line(p, o + vec2(-3.0, -8.0), o + vec2(3.0, -8.0));
    line(p, o + vec2(-3.0, 8.0), o + vec2(3.0, 8.0));
    line(p, o + vec2(-2.0, 3.0), o + vec2(2.0, 3.0));
}

/// The Blend tool: a crosshair with a square below right of the hotspot, hollow away from art,
/// filled over an object; over an anchor point a ringed dot (the blend starts there).
fn blend(p: &mut Glyph, o: Pos2, badge: Cursor) {
    crosshair(p, o);
    let b = o + vec2(9.0, 9.0);
    if badge == Cursor::BlendAnchor {
        p.circle_stroke(b + vec2(3.5, 3.5), 3.5, Stroke::new(3.0, HALO));
        p.circle_stroke(b + vec2(3.5, 3.5), 3.5, Stroke::new(1.2, INK));
        p.circle_filled(b + vec2(3.5, 3.5), 1.4, INK);
        return;
    }
    let fill = if badge == Cursor::BlendObject { INK } else { HALO };
    poly(p, vec![b, b + vec2(7.0, 0.0), b + vec2(7.0, 7.0), b + vec2(0.0, 7.0)], fill, INK);
}

/// Cursor `c`'s shapes with its hotspot at `p`, or `None` for cursors that stay system cursors
/// (hand, zoom, busy states).
fn shapes(c: Cursor, p: Pos2) -> Option<Vec<Shape>> {
    let mut glyph = Glyph::default();
    let painter = &mut glyph;
    match c {
        Cursor::Arrow => arrow(painter, p, false),
        Cursor::ArrowHollow => arrow(painter, p, true),
        Cursor::Move => {
            arrow(painter, p, false);
            double_arrow(painter, p + vec2(15.0, 18.0), vec2(1.0, 0.0));
        }
        Cursor::Crosshair | Cursor::Eyedropper => crosshair(painter, p),
        Cursor::ResizeH => double_arrow(painter, p, vec2(1.0, 0.0)),
        Cursor::ResizeV => double_arrow(painter, p, vec2(0.0, 1.0)),
        Cursor::ResizeNwSe => double_arrow(painter, p, vec2(1.0, 1.0)),
        Cursor::ResizeNeSw => double_arrow(painter, p, vec2(1.0, -1.0)),
        Cursor::Rotate => rotate(painter, p),
        Cursor::CornerRadius => corner_radius(painter, p),
        Cursor::Pen => pen(painter, p, "*"),
        Cursor::PenAdd => pen(painter, p, "+"),
        Cursor::PenDelete => pen(painter, p, "-"),
        Cursor::PenClose => pen(painter, p, "o"),
        Cursor::PenContinue => pen(painter, p, "/"),
        Cursor::Text => ibeam(painter, p),
        Cursor::AddStop => stop_badge(painter, p, true),
        Cursor::RemoveStop => stop_badge(painter, p, false),
        Cursor::Slice => slice(painter, p),
        Cursor::SliceSelect => {
            arrow(painter, p, false);
            slice_badge(painter, p + vec2(11.0, 14.0));
        }
        Cursor::Width => width(painter, p, ""),
        Cursor::WidthAdd => width(painter, p, "+"),
        Cursor::WidthPoint => width(painter, p, "point"),
        Cursor::Blend | Cursor::BlendObject | Cursor::BlendAnchor => blend(painter, p, c),
        _ => return None,
    }
    Some(glyph.0)
}

/// Paint cursor `c` at `p` on the given (foreground) painter. Returns false for cursors that should
/// stay system cursors (hand, zoom, busy states).
pub fn paint(painter: &Painter, c: Cursor, p: Pos2) -> bool {
    shapes(c, p).map(|s| painter.extend(s)).is_some()
}

/// Cursor `c` as an OS cursor bitmap for a display at `pixels_per_point`, or `None` for cursors
/// that stay system cursors.
pub fn bitmap(c: Cursor, pixels_per_point: f32) -> Option<CustomCursorImage> {
    if !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return None;
    }
    let shapes = shapes(c, Pos2::ZERO)?;
    // A point of margin keeps the halo's antialiased edge.
    let bounds = shapes.iter().fold(Rect::NOTHING, |r, s| r.union(s.visual_bounding_rect())).expand(1.0);
    if !bounds.is_finite() || bounds.is_negative() {
        return None;
    }
    let ppp = pixels_per_point.min(f32::from(MAX_BITMAP - 2) / bounds.size().max_elem());
    // Whole-pixel offset of the hotspot, so the glyph lands where it does at that scale on screen.
    let offset = (-bounds.min.to_vec2() * ppp).ceil();
    let side = |v: f32| (v * ppp + 1.0).ceil().clamp(1.0, f32::from(MAX_BITMAP)) as u16;
    let size = [side(bounds.width()), side(bounds.height())];

    let mut tessellator = Tessellator::new(ppp, TessellationOptions::default(), [1, 1], vec![]);
    let mut mesh = Mesh::default();
    for s in shapes {
        tessellator.tessellate_shape(s, &mut mesh);
    }
    let rgba = rasterize(&mesh, ppp, offset, size);
    let hotspot = [offset.x as u16, offset.y as u16];
    Some(CustomCursorImage { rgba: rgba.into(), size, hotspot })
}

/// Draw a tessellated mesh (premultiplied vertex colours, positions in points) into a straight
/// RGBA bitmap of `size` pixels, a point at `p` landing on pixel `p * ppp + offset`.
fn rasterize(mesh: &Mesh, ppp: f32, offset: egui::Vec2, size: [u16; 2]) -> Vec<u8> {
    let (w, h) = (usize::from(size[0]), usize::from(size[1]));
    let mut px = vec![[0.0f32; 4]; w * h];
    let edge = |a: Pos2, b: Pos2, p: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let vertex = |i: u32| mesh.vertices.get(i as usize);
    for &[ia, ib, ic] in mesh.indices.as_chunks::<3>().0 {
        let (Some(a), Some(b), Some(c)) = (vertex(ia), vertex(ib), vertex(ic)) else { continue };
        let (pa, pb, pc) = (a.pos * ppp + offset, b.pos * ppp + offset, c.pos * ppp + offset);
        let area = edge(pa, pb, pc);
        if !area.is_finite() || area.abs() < 1e-6 {
            continue;
        }
        let x0 = pa.x.min(pb.x).min(pc.x).floor().max(0.0) as usize;
        let y0 = pa.y.min(pb.y).min(pc.y).floor().max(0.0) as usize;
        let x1 = (pa.x.max(pb.x).max(pc.x).ceil().max(0.0) as usize).min(w);
        let y1 = (pa.y.max(pb.y).max(pc.y).ceil().max(0.0) as usize).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let p = pos2(x as f32 + 0.5, y as f32 + 0.5);
                let (wa, wb, wc) = (edge(pb, pc, p) / area, edge(pc, pa, p) / area, edge(pa, pb, p) / area);
                if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                    continue;
                }
                let src: [f32; 4] =
                    std::array::from_fn(|k| (wa * f32::from(a.color[k]) + wb * f32::from(b.color[k]) + wc * f32::from(c.color[k])) / 255.0);
                if let Some(dst) = px.get_mut(y * w + x) {
                    // Premultiplied "over", as the GPU blends egui's meshes.
                    *dst = std::array::from_fn(|k| src[k] + dst[k] * (1.0 - src[3]));
                }
            }
        }
    }
    px.iter()
        .flat_map(|&[r, g, b, a]| {
            let un = |v: f32| if a > 0.0 { (v / a).clamp(0.0, 1.0) * 255.0 } else { 0.0 };
            [un(r), un(g), un(b), a.clamp(0.0, 1.0) * 255.0].map(|v| v.round() as u8)
        })
        .collect()
}

/// [`bitmap`] for this display's scale, made once per cursor and scale: egui-winit keeps the OS
/// cursor while the bitmap is the same allocation.
pub fn os_image(ctx: &egui::Context, c: Cursor) -> Option<CustomCursorImage> {
    type Cache = Vec<(Cursor, u32, Option<CustomCursorImage>)>;
    let ppp = ctx.pixels_per_point();
    ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<Arc<std::sync::Mutex<Cache>>>(egui::Id::new("os-cursor-bitmaps")).clone();
        let Ok(mut cache) = cache.lock() else { return bitmap(c, ppp) };
        if let Some((.., img)) = cache.iter().find(|(k, s, _)| *k == c && *s == ppp.to_bits()) {
            return img.clone();
        }
        let img = bitmap(c, ppp);
        cache.push((c, ppp.to_bits(), img.clone()));
        img
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_hotspot_is_tip() {
        let pts = arrow_points(pos2(10.0, 20.0));
        assert_eq!(pts[0], pos2(10.0, 20.0));
        assert!(pts.iter().all(|q| q.x >= 10.0 && q.y >= 20.0));
    }

    const ALL: [Cursor; 32] = [
        Cursor::Arrow,
        Cursor::ArrowHollow,
        Cursor::Move,
        Cursor::Crosshair,
        Cursor::ResizeH,
        Cursor::ResizeV,
        Cursor::ResizeNwSe,
        Cursor::ResizeNeSw,
        Cursor::Rotate,
        Cursor::CornerRadius,
        Cursor::Pen,
        Cursor::PenAdd,
        Cursor::PenDelete,
        Cursor::PenClose,
        Cursor::PenContinue,
        Cursor::Text,
        Cursor::Hand,
        Cursor::HandGrab,
        Cursor::ZoomIn,
        Cursor::ZoomOut,
        Cursor::Eyedropper,
        Cursor::NotAllowed,
        Cursor::AddStop,
        Cursor::RemoveStop,
        Cursor::Slice,
        Cursor::SliceSelect,
        Cursor::Width,
        Cursor::WidthAdd,
        Cursor::WidthPoint,
        Cursor::Blend,
        Cursor::BlendObject,
        Cursor::BlendAnchor,
    ];

    fn alpha(img: &egui::CustomCursorImage, x: u16, y: u16) -> u8 {
        img.rgba[(usize::from(y) * usize::from(img.size[0]) + usize::from(x)) * 4 + 3]
    }

    /// Every glyph becomes an OS cursor bitmap at any scale: the right length, its hotspot inside
    /// it, not too big for the OS, and some ink; the system-only cursors stay system cursors.
    #[test]
    fn glyph_cursors_have_bitmaps() {
        let painter_ctx = egui::Context::default();
        for c in ALL {
            let painted = paint(&egui::Painter::new(painter_ctx.clone(), egui::LayerId::background(), egui::Rect::EVERYTHING), c, pos2(50.0, 50.0));
            for ppp in [1.0, 1.5, 2.0] {
                let img = bitmap(c, ppp);
                assert_eq!(img.is_some(), painted, "{c:?} at {ppp}x");
                let Some(img) = img else { continue };
                let [w, h] = img.size;
                assert_eq!(img.rgba.len(), usize::from(w) * usize::from(h) * 4, "{c:?}");
                assert!(img.hotspot[0] < w && img.hotspot[1] < h, "{c:?} hotspot {:?} in {w}x{h}", img.hotspot);
                assert!(w <= MAX_BITMAP && h <= MAX_BITMAP, "{c:?} is {w}x{h}");
                assert!(img.rgba.as_chunks::<4>().0.iter().any(|p| p[3] == 255), "{c:?} has opaque ink");
            }
        }
    }

    /// The arrow's hotspot is its tip: ink there, nothing up and left of it.
    #[test]
    fn arrow_bitmap_hotspot_is_tip() {
        for ppp in [1.0, 2.0] {
            let Some(img) = bitmap(Cursor::Arrow, ppp) else { panic!("arrow has a bitmap") };
            let [hx, hy] = img.hotspot;
            assert!(alpha(&img, hx, hy + 2) > 128, "ink just below the tip at {ppp}x");
            assert_eq!(alpha(&img, 0, 0), 0, "transparent corner at {ppp}x");
            // The halo and the antialiasing margin, 2.5 points.
            assert!(f32::from(hx.max(hy)) <= 2.5 * ppp + 1.0, "tip near the top left at {ppp}x: {:?}", img.hotspot);
        }
        let (one, two) = (bitmap(Cursor::Arrow, 1.0).map(|i| i.size), bitmap(Cursor::Arrow, 2.0).map(|i| i.size));
        let (Some(one), Some(two)) = (one, two) else { panic!("arrow has bitmaps") };
        assert!(two[0] >= one[0] * 2 - 2 && two[1] >= one[1] * 2 - 2, "2x is twice as big: {one:?} {two:?}");
    }

    /// The cache hands back the same bitmap, so egui-winit (which keys OS cursors by its pointer)
    /// doesn't rebuild the OS cursor every frame; another scale is another bitmap.
    #[test]
    fn os_images_are_cached() {
        let ctx = egui::Context::default();
        let (Some(a), Some(b)) = (os_image(&ctx, Cursor::Pen), os_image(&ctx, Cursor::Pen)) else { panic!("pen has a bitmap") };
        assert!(std::sync::Arc::ptr_eq(&a.rgba, &b.rgba));
        ctx.set_pixels_per_point(2.0);
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
        assert_eq!(ctx.pixels_per_point(), 2.0);
        let Some(c) = os_image(&ctx, Cursor::Pen) else { panic!("pen has a bitmap") };
        assert!(c.size[0] > a.size[0]);
        assert!(os_image(&ctx, Cursor::Hand).is_none());
    }
}
