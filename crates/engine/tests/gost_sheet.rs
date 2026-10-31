//! GOST building blocks (NSAI-8096 A2, A3): the GOST 2.307 dimension-style preset and the
//! TITLEBLOCK command (GOST 2.301 frame, GOST 2.104 form 1 title block with attribute fields).

use cadcraft_engine::Session;
use cadcraft_engine::doc::{DimStyle, EntityKind, Handle, TextStyle};
use serde_json::{Value, json};

fn metric() -> Session {
    let mut s = Session::new();
    s.execute("new", &json!({ "metric": true })).unwrap();
    s
}

#[test]
fn gost_dimstyle_preset() {
    let mut s = metric();
    let r = s.execute("dimstyle", &json!({ "preset": "gost" })).unwrap();
    assert_eq!(r["current"], "GOST 2.307", "{r}");
    let d = s.doc().unwrap();
    let st = d.dim_style("GOST 2.307").unwrap();
    assert_eq!(st.decimal_separator, ",");
    assert_eq!((st.text_above, st.text_inside_horizontal, st.text_outside_horizontal), (1, false, false));
    assert_eq!((st.arrow_block.as_str(), st.tick_size), ("", 0.0));
    assert_eq!(st.text_style, TextStyle::GOST_TYPE_B);
    // The text style it uses is created with it.
    assert!(d.text_style(TextStyle::GOST_TYPE_B).is_some());

    // Dimensions in it read with a comma.
    s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [12.5, 0], "at": [6, 10] })).unwrap();
    let d = s.doc().unwrap();
    let dim = d.model.iter().find_map(|e| if let EntityKind::Dimension(dm) = &e.kind { Some(dm.clone()) } else { None }).unwrap();
    assert_eq!(dim.style, "GOST 2.307");
    let g = cadcraft_engine::render::dimension_in(d, &dim);
    assert_eq!(g.value, "12,5");

    // Named, adjusted, and unknown presets.
    s.execute("dimstyle", &json!({ "preset": "GOST 2.307", "name": "Big", "DIMTXT": 5, "current": false })).unwrap();
    let st = s.doc().unwrap().dim_style("Big").unwrap().clone();
    assert_eq!((st.text_height, st.decimal_separator.as_str()), (5.0, ","));
    assert!(s.execute("dimstyle", &json!({ "preset": "ansi" })).is_err());
    assert_eq!(DimStyle::gost("X").name, "X");
}

fn attribs(s: &Session, h: &Value) -> Vec<(String, String)> {
    let h = Handle::parse_hex(h.as_str().unwrap()).unwrap();
    match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::Insert(i) => i.attribs.iter().map(|a| (a.tag.clone(), a.text.value.clone())).collect(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn titleblock_draws_an_a3_sheet_with_form_1() {
    let mut s = metric();
    let r = s
        .execute(
            "titleblock",
            &json!({ "format": "A3", "at": [1000, 0], "fields": { "designation": "АБВГ.123456.001", "NAME": "Вал", "SCALE": "1:2", "SHEETS": 1, "MATERIAL": "Круг В1-30 ГОСТ 2590-2006 / 45-б-Т ГОСТ 1050-2013" } }),
        )
        .unwrap();
    assert_eq!(r["block"], "GOST_A3_L_F1");
    assert_eq!(r["sheet"], json!([420.0, 297.0]));
    assert_eq!(r["frame"], json!([[1020.0, 5.0], [1415.0, 292.0]]));
    assert_eq!(r["titleBlock"], json!([[1230.0, 5.0], [1415.0, 60.0]]));
    let a = attribs(&s, &r["handle"]);
    assert_eq!(a.len(), 14);
    assert!(a.contains(&("DESIGNATION".into(), "АБВГ.123456.001".into())));
    assert!(a.contains(&("NAME".into(), "Вал".into())));
    assert!(a.contains(&("SHEETS".into(), "1".into())));
    assert!(a.contains(&("DEVELOPER".into(), String::new())));
    // A value too long for its cell is narrowed to fit; a short one keeps the style's width.
    let h = Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
    let EntityKind::Insert(ins) = &s.doc().unwrap().entity(h).unwrap().kind else { panic!() };
    let mat = ins.attribs.iter().find(|a| a.tag == "MATERIAL").unwrap();
    assert!(mat.text.width_factor < 0.9, "{}", mat.text.width_factor);
    let name = ins.attribs.iter().find(|a| a.tag == "NAME").unwrap();
    assert!((name.text.width_factor - 0.9).abs() < 1e-12);

    // The block holds the labels in the GOST type B style and the frame lines.
    let d = s.doc().unwrap();
    let b = d.block("GOST_A3_L_F1").unwrap();
    let labels: Vec<&str> = b.entities.iter().filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.value.as_str()) } else { None }).collect();
    for l in ["Разраб.", "Пров.", "Т.контр.", "Н.контр.", "Утв.", "Масса", "Масштаб", "Лит.", "№ докум."]
    {
        assert!(labels.contains(&l), "{l}");
    }
    assert!(b.entities.iter().all(|e| match &e.kind {
        EntityKind::Text(t) => t.style == TextStyle::GOST_TYPE_B,
        _ => true,
    }));

    // A second sheet reuses the definition; A4 is portrait and a bare frame has no title block.
    s.execute("titleblock", &json!({ "format": "A3", "at": [0, 0] })).unwrap();
    let r4 = s.execute("titleblock", &json!({ "format": "a4", "form": "none" })).unwrap();
    assert_eq!(r4["sheet"], json!([210.0, 297.0]));
    assert!(r4.get("titleBlock").is_none());
    assert_eq!(s.doc().unwrap().blocks.keys().filter(|k| k.starts_with("GOST_")).count(), 2);
}

#[test]
fn titleblock_rejects_bad_parameters() {
    let mut s = metric();
    for p in [
        json!({ "format": "A5" }),
        json!({ "format": 3 }),
        json!({ "orientation": "sideways" }),
        json!({ "form": "2b" }),
        json!({ "fields": { "NOPE": "x" } }),
        json!({ "fields": { "NAME": ["x"] } }),
        json!({ "fields": "NAME" }),
        json!({ "fields": { "NAME": "x".repeat(300) } }),
        json!({ "form": "none", "fields": { "NAME": "x" } }),
        json!({ "at": [f64::MAX, f64::MIN] }),
        json!({ "at": "here" }),
    ] {
        let _ = s.execute("titleblock", &p);
    }
    assert!(s.execute("titleblock", &json!({ "format": "A5" })).is_err());
    assert!(s.execute("titleblock", &json!({ "fields": { "NOPE": "x" } })).is_err());
}
