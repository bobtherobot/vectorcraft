//! Layer Options (double-click a layer in the Layers panel, or the panel menu): the layer's name,
//! its colour (a preset or a custom colour; selections on the layer are drawn in it), and
//! Template, Show, Lock and Print. A template layer is locked and doesn't print, so Lock and Print
//! are disabled while Template is on. OK runs `layer.setProps` as one undo step.
//!
//! Fields: `id`, `name`, `color` (`#rrggbb`, or a preset's name such as "Light Blue"), `template`,
//! `visible`, `locked`, `printable`.

use serde_json::{Value, json};
use vectorcraft_color::Color;
use vectorcraft_doc::{NodeId, NodeKind};

use super::swatch_options::{grid, label};
use super::tile_edge_color::{layer_color_picker, resolve};
use super::{DialogSpec, form, run_and_close};
use crate::state::Dialog;
use crate::{VectorcraftApp, widgets};

/// The dialog kind of Layer Options.
pub const KIND: &str = "layerOptions";

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("Layer Options").into(), body, confirm, min_width: 320.0, ..DialogSpec::FORM };

/// Open Layer Options on layer `id` (default: the current layer).
pub fn open(app: &mut VectorcraftApp, id: Option<u64>) -> Result<Value, String> {
    let st = app.session.active().ok_or("no document")?;
    let id = id.map(NodeId).or(st.active_layer).ok_or("no current layer")?;
    let n = st.doc.node(id).ok_or("no such layer")?;
    let NodeKind::Layer { template, printable, .. } = &n.kind else { return Err("not a layer".into()) };
    let [r, g, b] = st.doc.layer_color(id);
    let fields = json!({
        "id": id.0,
        "name": n.display_name(),
        "color": Color::rgb8(r, g, b).to_hex(),
        "template": template,
        "visible": n.visible,
        "locked": n.locked,
        "printable": printable,
    });
    app.ui.dialog = Some(Dialog::new(KIND, fields));
    Ok(Value::Null)
}

fn body(_: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    grid(ui, |ui| {
        label(ui, tl!("Name:"));
        form::text(ui, d, "name", 190.0);
        ui.end_row();
        label(ui, tl!("Color:"));
        layer_color_picker(ui, d, "color");
        ui.end_row();
    });
    ui.add_space(8.0);
    let template = d.bool("template");
    // (key, label, enabled): a template layer is always locked and never prints.
    let options = [("template", "Template", true), ("visible", "Show", true), ("locked", "Lock", !template), ("printable", "Print", !template)];
    for (key, text, enabled) in options {
        let on = d.bool(key);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            if widgets::check(ui, text, on, enabled) {
                d.fields.insert(key.into(), json!(!on));
            }
        });
    }
    false
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let id = d.fields.get("id").and_then(Value::as_u64).ok_or("no layer")?;
    let color = d.str("color");
    let hex = resolve(&color).ok_or_else(|| format!("`{color}` is not a colour (#rrggbb or a preset name)"))?.to_hex();
    let template = d.bool("template");
    let mut params = json!({
        "id": id,
        "name": d.str("name"),
        "color": hex,
        "template": template,
        "visible": d.bool("visible"),
    });
    // Lock and Print stay as they were while Template is on.
    if !template {
        params["locked"] = json!(d.bool("locked"));
        params["printable"] = json!(d.bool("printable"));
    }
    run_and_close(app, "layer.setProps", params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_doc::LayerColor;
    use vectorcraft_engine::Session;

    fn layer(app: &VectorcraftApp, id: u64) -> vectorcraft_doc::Node {
        (*app.session.active().unwrap().doc.node(NodeId(id)).unwrap()).clone()
    }

    #[test]
    fn layer_options_set_name_colour_and_options_in_one_step() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({})).unwrap();
        let id = app.run("layer.new", json!({"name": "Ink"})).unwrap()["id"].as_u64().unwrap();
        app.run("ui.layerOptions", json!({"id": id})).unwrap();
        let d = app.ui.dialog.as_ref().unwrap();
        assert_eq!(d.kind, KIND);
        assert_eq!(d.str("name"), "Ink");
        assert!(d.bool("visible") && d.bool("printable") && !d.bool("locked"));
        // Drawn headlessly in the shared frame.
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut out = ctx.run_ui(Default::default(), |ui| super::super::show(&mut app, ui.ctx()));
        out.textures_delta.clear();
        let d = app.ui.dialog.as_mut().unwrap();
        d.fields.insert("name".into(), json!("Sketch"));
        d.fields.insert("color".into(), json!("#123456"));
        d.fields.insert("printable".into(), json!(false));
        super::super::confirm(&mut app).unwrap();
        assert!(app.ui.dialog.is_none());
        let n = layer(&app, id);
        assert_eq!(n.name.as_deref(), Some("Sketch"));
        let NodeKind::Layer { color, printable, .. } = n.kind else { panic!() };
        assert_eq!(color, LayerColor::Custom([0x12, 0x34, 0x56]));
        assert!(!printable);
        // One undo step brings it all back.
        app.run("edit.undo", json!({})).unwrap();
        let n = layer(&app, id);
        assert_eq!(n.name.as_deref(), Some("Ink"));
        assert!(matches!(n.kind, NodeKind::Layer { printable: true, color: LayerColor::Preset(_), .. }));
    }

    #[test]
    fn layer_options_default_to_the_current_layer_and_refuse_objects() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({})).unwrap();
        app.run("ui.layerOptions", json!({})).unwrap();
        let current = app.session.active().unwrap().active_layer.unwrap();
        assert_eq!(app.ui.dialog.as_ref().unwrap().fields["id"], json!(current.0));
        app.ui.dialog = None;
        let rect = app.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap()["id"].clone();
        assert!(app.run("ui.layerOptions", json!({"id": rect})).is_err());
        assert!(app.ui.dialog.is_none());
    }
}
