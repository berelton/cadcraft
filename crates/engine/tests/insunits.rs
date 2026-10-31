//! Drawing units across files (NSAI-8096 A4): WBLOCK writes a metric drawing as metric, and
//! INSERT of a file, XATTACH and IMPORT scale the file's content from its `$INSUNITS` to this
//! drawing's, as AutoCAD does (1 when either side is unitless).

use std::path::PathBuf;

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_engine::doc::geom::Vec3;
use cadcraft_engine::doc::{Drawing, Entity, EntityKind, Line, insunits_factor};
use serde_json::{Value, json};

fn hooks() {
    set_io(IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: None,
    });
}

fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cadcraft-insunits-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A millimetre drawing holding one 0,0–254,0 line (10 inches), saved as `name` in `dir`.
fn mm_part(dir: &std::path::Path, name: &str) -> String {
    let mut d = Drawing::new_metric();
    let h = d.new_handle();
    d.model.push(Entity::new(h, EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(254.0, 0.0, 0.0) })));
    let path = dir.join(name);
    std::fs::write(&path, cadcraft_io::write(&d, &path.to_string_lossy()).unwrap()).unwrap();
    path.to_string_lossy().to_string()
}

fn insert_scale(s: &Session, handle: &Value) -> Vec3 {
    let h = cadcraft_engine::doc::Handle::parse_hex(handle.as_str().unwrap()).unwrap();
    match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::Insert(i) => i.scale,
        other => panic!("{other:?}"),
    }
}

fn inch_session() -> Session {
    let mut s = Session::new();
    s.execute("new", &json!({ "metric": false })).unwrap();
    assert_eq!(s.doc().unwrap().header.i64("INSUNITS", 0), 1);
    s
}

#[test]
fn unit_factors() {
    assert_eq!(insunits_factor(4, 4), 1.0);
    assert!((insunits_factor(4, 1) - 1.0 / 25.4).abs() < 1e-15);
    assert!((insunits_factor(1, 4) - 25.4).abs() < 1e-12);
    assert!((insunits_factor(6, 4) - 1000.0).abs() < 1e-9);
    assert!((insunits_factor(2, 1) - 12.0).abs() < 1e-12);
    for unitless in [(0, 4), (4, 0), (0, 0), (99, 4), (4, -1), (i64::MAX, i64::MIN)] {
        assert_eq!(insunits_factor(unitless.0, unitless.1), 1.0, "{unitless:?}");
    }
}

#[test]
fn wblock_keeps_metric_units() {
    hooks();
    let dir = folder("wblock");
    let mut s = Session::new();
    s.execute("new", &json!({ "metric": true })).unwrap();
    s.execute("units", &json!({ "luprec": 2 })).unwrap();
    let h = s.execute("line", &json!({ "points": [[0, 0], [100, 0]] })).unwrap()["handles"][0].clone();
    let path = dir.join("part.dxf");
    s.execute("wblock", &json!({ "path": path.to_string_lossy(), "handles": [h] })).unwrap();
    let back = cadcraft_io::read(&std::fs::read(&path).unwrap(), "part.dxf").unwrap();
    assert_eq!(back.header.i64("MEASUREMENT", -1), 1);
    assert_eq!(back.header.i64("INSUNITS", -1), 4);
    assert_eq!(back.header.i64("LUPREC", -1), 2);

    // A block is written in the drawing's units (it has none of its own).
    s.execute("block", &json!({ "name": "P", "base": [0, 0], "handles": [h], "keep": "delete" })).unwrap();
    let path2 = dir.join("p.dxf");
    s.execute("wblock", &json!({ "path": path2.to_string_lossy(), "name": "P" })).unwrap();
    let back = cadcraft_io::read(&std::fs::read(&path2).unwrap(), "p.dxf").unwrap();
    assert_eq!(back.header.i64("INSUNITS", -1), 4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn insert_xattach_and_import_scale_to_this_drawings_units() {
    hooks();
    let dir = folder("insert");
    let part = mm_part(&dir, "part.dxf");

    // INSERT of a millimetre file into an inch drawing: 254 mm come in as 10 inches.
    let mut s = inch_session();
    let r = s.execute("insert", &json!({ "file": part, "at": [0, 0] })).unwrap();
    let k = insert_scale(&s, &r["handle"]);
    assert!((k.x - 1.0 / 25.4).abs() < 1e-12 && (k.y - 1.0 / 25.4).abs() < 1e-12, "{k:?}");
    // On top of the scale asked for.
    let r = s.execute("insert", &json!({ "name": "part", "at": [0, 0], "scale": 2 })).unwrap();
    assert!((insert_scale(&s, &r["handle"]).x - 2.0 / 25.4).abs() < 1e-12);

    // XATTACH likewise.
    let r = s.execute("xattach", &json!({ "path": part, "name": "X", "at": [0, 0] })).unwrap();
    assert!((insert_scale(&s, &r["handle"]).x - 1.0 / 25.4).abs() < 1e-12);

    // IMPORT copies the geometry at this drawing's size, unless asked not to.
    let mut s = inch_session();
    let r = s.execute("import", &json!({ "path": part })).unwrap();
    let h = cadcraft_engine::doc::Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::Line(l) => assert!((l.b.x - 10.0).abs() < 1e-9, "{l:?}"),
        other => panic!("{other:?}"),
    }
    let r = s.execute("import", &json!({ "path": part, "matchUnits": false })).unwrap();
    let h = cadcraft_engine::doc::Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::Line(l) => assert!((l.b.x - 254.0).abs() < 1e-9, "{l:?}"),
        other => panic!("{other:?}"),
    }

    // Same units, or a unitless target: scale 1.
    let mut s = Session::new();
    s.execute("new", &json!({ "metric": true })).unwrap();
    let r = s.execute("insert", &json!({ "file": part, "at": [0, 0] })).unwrap();
    assert_eq!(insert_scale(&s, &r["handle"]).x, 1.0);
    s.execute("units", &json!({ "insunits": 0 })).unwrap();
    let r = s.execute("insert", &json!({ "name": "part", "at": [0, 0] })).unwrap();
    assert_eq!(insert_scale(&s, &r["handle"]).x, 1.0);
    let _ = std::fs::remove_dir_all(&dir);
}
