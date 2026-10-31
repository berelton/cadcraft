//! TITLEBLOCK: a drawing sheet's frame and main title block, drawn to GOST 2.301 (sheet sizes,
//! a 20 mm binding margin and 5 mm elsewhere) and GOST 2.104 (form 1, 185 × 55 mm, for drawings).
//!
//! The sheet is a block (`GOST_A3_L_F1`, …) made once per format, orientation and form: the trim
//! border, the frame, the title block grid and its labels in the GOST 2.304 type B text style,
//! and attribute definitions for the fields. A reference to it is inserted with the field values
//! as attributes, so a program fills and reads the title block by tag (`texts` and `attedit`).
//! Sizes are millimetres; in a drawing in other units the reference is scaled by INSERT's units
//! rule like any block.

use std::sync::Arc;

use cadcraft_doc::{Attrib, Block, Common, Entity, EntityKind, HAlign, Line, Lineweight, Text, TextStyle, VAlign};
use cadcraft_geom::{Vec2, Vec3};
use serde_json::{Value, json};

use super::blocks::insert_with;
use super::{CommandSpec, bad, point_param, str_param};
use crate::{Result, Session};

pub(super) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("titleblock", "GOST Sheet Frame and Title Block", run_titleblock).params(PARAMS)]
}

const PARAMS: &str = "{format?: A4|A3|A2|A1|A0 (default A3), orientation?: portrait|landscape (default portrait for A4, landscape otherwise), form?: \"1\" (GOST 2.104 form 1, default) | \"none\" (frame only), at?: [x,y] (sheet's lower-left corner, default 0,0), fields?: {DESIGNATION, NAME, MATERIAL, ORG, LITERA, MASS, SCALE, SHEET, SHEETS, DEVELOPER, CHECKER, TCONTROL, NCONTROL, APPROVER: text}} → {handle, block, sheet: [w,h], frame: [[x,y],[x,y]], titleBlock?: [[x,y],[x,y]]}";

/// GOST 2.301 sheet sizes (short × long side, mm).
const FORMATS: [(&str, f64, f64); 5] =
    [("A4", 210.0, 297.0), ("A3", 297.0, 420.0), ("A2", 420.0, 594.0), ("A1", 594.0, 841.0), ("A0", 841.0, 1189.0)];

/// Frame margins: 20 mm on the left for binding, 5 mm on the other sides.
const LEFT: f64 = 20.0;
const MARGIN: f64 = 5.0;
/// Form 1 is 185 × 55 mm.
const FORM1: (f64, f64) = (185.0, 55.0);

/// Line weights: solid main lines (frame, title block outline) and thin lines.
const THICK: Lineweight = Lineweight::Mm100(50);
const THIN: Lineweight = Lineweight::Mm100(18);

/// One field of form 1: tag, prompt, centre in title-block coordinates, text height, and the
/// width its cell leaves for the text (mm).
struct Field {
    tag: &'static str,
    prompt: &'static str,
    at: (f64, f64),
    height: f64,
    room: f64,
}

const FIELDS: [Field; 14] = [
    Field { tag: "DESIGNATION", prompt: "Document designation", at: (125.0, 47.5), height: 7.0, room: 116.0 },
    Field { tag: "NAME", prompt: "Product name", at: (100.0, 27.5), height: 5.0, room: 66.0 },
    Field { tag: "MATERIAL", prompt: "Material", at: (100.0, 7.5), height: 5.0, room: 66.0 },
    Field { tag: "ORG", prompt: "Organization", at: (160.0, 7.5), height: 3.5, room: 46.0 },
    Field { tag: "LITERA", prompt: "Litera", at: (142.5, 27.5), height: 3.5, room: 14.0 },
    Field { tag: "MASS", prompt: "Mass", at: (158.5, 27.5), height: 3.5, room: 15.0 },
    Field { tag: "SCALE", prompt: "Scale", at: (176.0, 27.5), height: 3.5, room: 16.0 },
    Field { tag: "SHEET", prompt: "Sheet", at: (149.0, 17.5), height: 2.5, room: 9.0 },
    Field { tag: "SHEETS", prompt: "Sheets", at: (175.0, 17.5), height: 2.5, room: 17.0 },
    Field { tag: "DEVELOPER", prompt: "Developed by", at: (28.5, 27.5), height: 2.5, room: 21.0 },
    Field { tag: "CHECKER", prompt: "Checked by", at: (28.5, 22.5), height: 2.5, room: 21.0 },
    Field { tag: "TCONTROL", prompt: "Technical control", at: (28.5, 17.5), height: 2.5, room: 21.0 },
    Field { tag: "NCONTROL", prompt: "Standards control", at: (28.5, 7.5), height: 2.5, room: 21.0 },
    Field { tag: "APPROVER", prompt: "Approved by", at: (28.5, 2.5), height: 2.5, room: 21.0 },
];

/// Form 1 labels: text, point, centred (else left-aligned at the point), the width the cell
/// leaves for it (mm); all 2.5 mm high.
const LABELS: [(&str, (f64, f64), bool, f64); 15] = [
    ("Изм.", (3.5, 32.5), true, 6.0),
    ("Лист", (12.0, 32.5), true, 9.0),
    ("№ докум.", (28.5, 32.5), true, 22.0),
    ("Подп.", (47.5, 32.5), true, 14.0),
    ("Дата", (60.0, 32.5), true, 9.0),
    ("Разраб.", (1.0, 27.5), false, 15.0),
    ("Пров.", (1.0, 22.5), false, 15.0),
    ("Т.контр.", (1.0, 17.5), false, 15.0),
    ("Н.контр.", (1.0, 7.5), false, 15.0),
    ("Утв.", (1.0, 2.5), false, 15.0),
    ("Лит.", (142.5, 37.5), true, 14.0),
    ("Масса", (158.5, 37.5), true, 16.0),
    ("Масштаб", (176.0, 37.5), true, 17.0),
    ("Лист", (136.0, 17.5), false, 9.0),
    ("Листов", (156.0, 17.5), false, 10.0),
];

/// Form 1 grid lines in title-block coordinates: from, to, solid main line.
fn form1_lines() -> Vec<((f64, f64), (f64, f64), bool)> {
    let mut v = vec![
        // Outline.
        ((0.0, 0.0), (185.0, 0.0), true),
        ((185.0, 0.0), (185.0, 55.0), true),
        ((185.0, 55.0), (0.0, 55.0), true),
        ((0.0, 55.0), (0.0, 0.0), true),
        // Left part: columns 7, 10, 23, 15, 10 mm (Изм. and Лист merge below the header row).
        ((7.0, 30.0), (7.0, 55.0), true),
        ((17.0, 0.0), (17.0, 55.0), true),
        ((40.0, 0.0), (40.0, 55.0), true),
        ((55.0, 0.0), (55.0, 55.0), true),
        ((65.0, 0.0), (65.0, 55.0), true),
        // Right part: designation (15 mm high), name / material, and the 50 mm column.
        ((65.0, 40.0), (185.0, 40.0), true),
        ((65.0, 15.0), (185.0, 15.0), true),
        ((135.0, 0.0), (135.0, 40.0), true),
        ((135.0, 35.0), (185.0, 35.0), true),
        ((135.0, 20.0), (185.0, 20.0), true),
        ((150.0, 20.0), (150.0, 40.0), true),
        ((167.0, 20.0), (167.0, 40.0), true),
        ((140.0, 20.0), (140.0, 35.0), false),
        ((145.0, 20.0), (145.0, 35.0), false),
        ((155.0, 15.0), (155.0, 20.0), false),
    ];
    // Rows of 5 mm in the left part; the header row's lines are main lines.
    for i in 1..11 {
        let y = f64::from(i) * 5.0;
        v.push(((0.0, y), (65.0, y), i == 6 || i == 7));
    }
    v
}

fn line(a: Vec2, b: Vec2) -> EntityKind {
    EntityKind::Line(Line { a: Vec3::new(a.x, a.y, 0.0), b: Vec3::new(b.x, b.y, 0.0) })
}

fn text(at: Vec2, value: &str, height: f64, centred: bool, style: &TextStyle) -> Text {
    Text {
        insert: Vec3::new(at.x, at.y, 0.0),
        align_pt: Some(Vec3::new(at.x, at.y, 0.0)),
        height,
        value: value.into(),
        rotation: 0.0,
        width_factor: style.width_factor,
        oblique: style.oblique,
        style: style.name.clone(),
        halign: if centred { HAlign::Center } else { HAlign::Left },
        valign: VAlign::Middle,
    }
}

/// The width factor that fits `value` (at `height`, `wf`) into `room` mm: `wf` when it fits,
/// narrower when not, never below 0.3.
fn fit_width(value: &str, height: f64, wf: f64, room: f64) -> f64 {
    let w = cadcraft_fonts::line_width(value, height, wf);
    if w > room && w.is_finite() { (wf * room / w).max(0.3) } else { wf }
}

/// The sheet size (width, height) of `format` in `orientation`.
fn sheet_size(format: &str, orientation: Option<&str>) -> Result<(String, f64, f64, bool)> {
    let (name, short, long) = FORMATS
        .iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(format.trim()))
        .copied()
        .ok_or_else(|| bad("titleblock", format!("unknown format `{format}` (A4, A3, A2, A1, A0)")))?;
    let landscape = match orientation.map(|o| o.trim().to_ascii_lowercase()) {
        None => name != "A4",
        Some(o) if o == "landscape" || o == "l" => true,
        Some(o) if o == "portrait" || o == "p" => false,
        Some(o) => return Err(bad("titleblock", format!("unknown orientation `{o}` (portrait, landscape)"))),
    };
    let (w, h) = if landscape { (long, short) } else { (short, long) };
    Ok((name.to_string(), w, h, landscape))
}

/// The block that draws the sheet, defined when the drawing doesn't have it yet.
fn define_sheet(d: &mut cadcraft_doc::Drawing, name: &str, w: f64, h: f64, form1: bool, style: &TextStyle) {
    if d.block(name).is_some() {
        return;
    }
    let mut b = Block::new(name);
    b.units = 4;
    let add = |b: &mut Block, d: &mut cadcraft_doc::Drawing, kind: EntityKind, lw: Lineweight| {
        let mut e = Entity::new(d.new_handle(), kind);
        e.common = Common { layer: "0".into(), lineweight: lw, ..Common::default() };
        b.entities.push(e);
    };
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
        [(x0, y0, x1, y0), (x1, y0, x1, y1), (x1, y1, x0, y1), (x0, y1, x0, y0)].map(|(a, b, c, e)| (Vec2::new(a, b), Vec2::new(c, e)))
    };
    // Trim border (thin) and frame (main line).
    for (a, c) in rect(0.0, 0.0, w, h) {
        add(&mut b, d, line(a, c), THIN);
    }
    for (a, c) in rect(LEFT, MARGIN, w - MARGIN, h - MARGIN) {
        add(&mut b, d, line(a, c), THICK);
    }
    if form1 {
        let o = Vec2::new(w - MARGIN - FORM1.0, MARGIN);
        let at = |p: (f64, f64)| o + Vec2::new(p.0, p.1);
        for (a, c, thick) in form1_lines() {
            add(&mut b, d, line(at(a), at(c)), if thick { THICK } else { THIN });
        }
        for (label, p, centred, room) in LABELS {
            let mut t = text(at(p), label, 2.5, centred, style);
            t.width_factor = fit_width(label, t.height, t.width_factor, room);
            add(&mut b, d, EntityKind::Text(t), Lineweight::ByLayer);
        }
        for f in &FIELDS {
            let a = Attrib {
                tag: f.tag.into(),
                text: text(at(f.at), "", f.height, true, style),
                invisible: false,
                constant: false,
                prompt: f.prompt.into(),
                props: Default::default(),
                multiline: None,
                modes: Default::default(),
            };
            add(&mut b, d, EntityKind::AttDef(a), Lineweight::ByLayer);
        }
    }
    d.blocks.insert(name.to_string(), Arc::new(b));
}

fn run_titleblock(s: &mut Session, p: &Value) -> Result<Value> {
    let (format, w, h, landscape) = sheet_size(str_param(p, "format").unwrap_or("A3"), str_param(p, "orientation"))?;
    let form1 = match str_param(p, "form").map(str::trim) {
        None | Some("1") => true,
        Some(f) if f.eq_ignore_ascii_case("none") => false,
        Some(f) => return Err(bad("titleblock", format!("unknown form `{f}` (1, none)"))),
    };
    let fields = match p.get("fields") {
        None | Some(Value::Null) => serde_json::Map::new(),
        Some(Value::Object(o)) => {
            let mut out = serde_json::Map::new();
            for (k, v) in o {
                let tag = k.trim().to_ascii_uppercase();
                if !FIELDS.iter().any(|f| f.tag == tag) {
                    return Err(bad("titleblock", format!("unknown field `{k}`")));
                }
                let v = match v {
                    Value::String(t) => t.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => return Err(bad("titleblock", format!("field `{k}` must be text"))),
                };
                if v.chars().count() > 256 {
                    return Err(bad("titleblock", format!("field `{k}` is longer than 256 characters")));
                }
                out.insert(tag, Value::String(v));
            }
            out
        }
        Some(_) => return Err(bad("titleblock", "`fields` must be an object of tag → text")),
    };
    if !form1 && !fields.is_empty() {
        return Err(bad("titleblock", "`fields` need a title block (form 1)"));
    }
    let at = point_param(p, "at").unwrap_or(Vec2::ZERO);
    if !(at.x.is_finite() && at.y.is_finite()) {
        return Err(bad("titleblock", "`at` must be a point"));
    }
    let block = format!("GOST_{format}_{}_{}", if landscape { "L" } else { "P" }, if form1 { "F1" } else { "F0" });
    let d = s.doc_mut()?;
    let style = match d.text_style(TextStyle::GOST_TYPE_B) {
        Some(t) => t.clone(),
        None => {
            let t = TextStyle::gost_type_b(TextStyle::GOST_TYPE_B);
            d.text_styles.push(t.clone());
            t
        }
    };
    define_sheet(d, &block, w, h, form1, &style);
    let handle = insert_with(s, &block, at, Vec2::new(1.0, 1.0), 0.0, None, &fields)?;
    // A value wider than its cell is narrowed to fit (down to a width factor of 0.3).
    s.doc_mut()?.modify_entity(handle, |e| {
        if let EntityKind::Insert(i) = &mut e.kind {
            for a in &mut i.attribs {
                if let Some(f) = FIELDS.iter().find(|f| f.tag == a.tag) {
                    a.text.width_factor = fit_width(&a.text.value, a.text.height, a.text.width_factor, f.room);
                }
            }
        }
    })?;
    let mut out = json!({
        "handle": handle.hex(),
        "block": block,
        "sheet": [w, h],
        "frame": [[at.x + LEFT, at.y + MARGIN], [at.x + w - MARGIN, at.y + h - MARGIN]],
    });
    if form1 && let Some(o) = out.as_object_mut() {
        let x0 = at.x + w - MARGIN - FORM1.0;
        o.insert("titleBlock".into(), json!([[x0, at.y + MARGIN], [x0 + FORM1.0, at.y + MARGIN + FORM1.1]]));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form1_grid_stays_inside_185_by_55() {
        for (a, b, _) in form1_lines() {
            for (x, y) in [a, b] {
                assert!((0.0..=FORM1.0).contains(&x) && (0.0..=FORM1.1).contains(&y), "{x},{y}");
            }
        }
        for f in &FIELDS {
            assert!(f.at.0 > 0.0 && f.at.0 < FORM1.0 && f.at.1 > 0.0 && f.at.1 < FORM1.1, "{}", f.tag);
        }
    }

    #[test]
    fn sheet_sizes() {
        assert_eq!(sheet_size("a3", None).ok().map(|s| (s.1, s.2)), Some((420.0, 297.0)));
        assert_eq!(sheet_size("A4", None).ok().map(|s| (s.1, s.2)), Some((210.0, 297.0)));
        assert_eq!(sheet_size("A1", Some("portrait")).ok().map(|s| (s.1, s.2)), Some((594.0, 841.0)));
        assert!(sheet_size("A5", None).is_err());
        assert!(sheet_size("A3", Some("diagonal")).is_err());
    }
}
