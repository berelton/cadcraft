//! TEXTS reads what a drawing says (NSAI-8082 B2/B3): `%%` codes decoded, texts inside block
//! definitions (a title block's labels) at their place in the space, and dimension text.

use cadcraft_engine::Session;
use serde_json::{Value, json};

fn texts(s: &mut Session, p: Value) -> Vec<Value> {
    s.execute("texts", &p).unwrap()["texts"].as_array().cloned().unwrap_or_default()
}

fn find<'a>(all: &'a [Value], text: &str) -> &'a Value {
    all.iter().find(|t| t["text"] == text).unwrap_or_else(|| panic!("no {text:?} in {all:#?}"))
}

#[test]
fn percent_codes_are_decoded() {
    let mut s = Session::new();
    s.execute("text", &json!({ "at": [0, 0], "text": "%%c20 %%p0.1 45%%d %%uA%%u 5%%%" })).unwrap();
    s.execute("mtext", &json!({ "at": [0, 5], "text": "%%c8 H7" })).unwrap();
    let all = texts(&mut s, json!({}));
    let t = find(&all, "⌀20 ±0.1 45° A 5%");
    assert_eq!(t["raw"], "%%c20 %%p0.1 45%%d %%uA%%u 5%%%");
    assert_eq!(t["type"], "Text");
    find(&all, "⌀8 H7");
    // Searching matches the decoded text.
    assert_eq!(texts(&mut s, json!({ "contains": "⌀20" })).len(), 1);
}

#[test]
fn texts_in_block_definitions_are_reported_where_they_are_drawn() {
    let mut s = Session::new();
    let a = s.execute("text", &json!({ "at": [1, 1], "text": "Разраб." })).unwrap()["handle"].as_str().unwrap().to_string();
    let b = s.execute("mtext", &json!({ "at": [1, 3], "text": "Лит." })).unwrap()["handle"].as_str().unwrap().to_string();
    s.execute("block", &json!({ "name": "Stamp", "base": [0, 0], "handles": [a, b], "keep": "delete" })).unwrap();
    let ins = s.execute("insert", &json!({ "name": "Stamp", "at": [100, 10], "scale": 2 })).unwrap()["handle"].as_str().unwrap().to_string();
    // Nested one level deeper: a block holding a reference to Stamp.
    s.execute("block", &json!({ "name": "Sheet", "base": [0, 0], "handles": [ins.clone()], "keep": "delete" })).unwrap();
    let sheet = s.execute("insert", &json!({ "name": "Sheet", "at": [0, 50] })).unwrap()["handle"].as_str().unwrap().to_string();

    let all = texts(&mut s, json!({}));
    assert_eq!(all.len(), 2, "{all:#?}");
    let t = find(&all, "Разраб.");
    assert_eq!(t["handle"], sheet);
    assert_eq!(t["block"], "Sheet/Stamp");
    assert!(t["entity"].is_string());
    let at = t["at"].as_array().unwrap();
    assert!((at[0].as_f64().unwrap() - 102.0).abs() < 1e-9 && (at[1].as_f64().unwrap() - 62.0).abs() < 1e-9, "{at:?}");
    find(&all, "Лит.");
    // The window works on the drawn position; `blocks: false` keeps to the space's own texts.
    assert_eq!(texts(&mut s, json!({ "window": [[101, 61], [103, 63]] })).len(), 1);
    assert!(texts(&mut s, json!({ "blocks": false })).is_empty());
    assert!(s.execute("texts", &json!({ "blocks": "yes" })).is_err());
}

#[test]
fn block_recursion_is_bounded() {
    // A block that references itself (as a damaged file can hold) ends at MAX_BLOCK_DEPTH.
    let mut s = Session::new();
    let t = s.execute("text", &json!({ "at": [0, 0], "text": "loop" })).unwrap()["handle"].as_str().unwrap().to_string();
    s.execute("block", &json!({ "name": "L", "base": [0, 0], "handles": [t], "keep": "delete" })).unwrap();
    s.execute("insert", &json!({ "name": "L", "at": [0, 0] })).unwrap();
    let mut d = s.doc().unwrap().clone();
    if let Some(b) = d.blocks.get_mut("L") {
        let b = std::sync::Arc::make_mut(b);
        let ins = d.model.iter().next().unwrap().as_ref().clone();
        let mut e = ins;
        e.handle = cadcraft_engine::doc::Handle(0xFFFF);
        b.entities.push(e);
    }
    *s.doc_mut().unwrap() = d;
    let all = texts(&mut s, json!({ "limit": 1000 }));
    assert!(!all.is_empty() && all.len() <= cadcraft_engine::doc::MAX_BLOCK_DEPTH + 1, "{}", all.len());
}

#[test]
fn dimension_text_is_reported_with_its_measurement() {
    let mut s = Session::new();
    s.execute("dimlinear", &json!({ "p1": [0, 0], "p2": [12.5, 0], "at": [6, 3] })).unwrap();
    s.execute("dimlinear", &json!({ "p1": [0, 10], "p2": [20, 10], "at": [10, 13], "text": "%%c<> H7" })).unwrap();
    let all = texts(&mut s, json!({}));
    let d = all.iter().filter(|t| t["type"] == "Dimension").collect::<Vec<_>>();
    assert_eq!(d.len(), 2, "{all:#?}");
    assert!((d[0]["measurement"].as_f64().unwrap() - 12.5).abs() < 1e-9);
    assert!(d[0]["text"].as_str().unwrap().starts_with("12.5"), "{}", d[0]["text"]);
    let t = d[1]["text"].as_str().unwrap();
    assert!(t.starts_with('⌀') && t.ends_with(" H7") && t.contains("20"), "{t}");
    assert!(texts(&mut s, json!({ "dimensions": false })).is_empty());
}

#[test]
fn nested_fan_out_is_bounded() {
    // Eight levels of blocks, each holding ten references to the next: 10^8 texts if followed
    // to the end. TEXTS stays within its visit budget and returns promptly.
    let mut s = Session::new();
    let t = s.execute("text", &json!({ "at": [0, 0], "text": "leaf" })).unwrap()["handle"].as_str().unwrap().to_string();
    s.execute("block", &json!({ "name": "B0", "base": [0, 0], "handles": [t], "keep": "delete" })).unwrap();
    for level in 1..8 {
        let mut hs = Vec::new();
        for i in 0..10 {
            let r = s.execute("insert", &json!({ "name": format!("B{}", level - 1), "at": [i, 0] })).unwrap();
            hs.push(r["handle"].as_str().unwrap().to_string());
        }
        s.execute("block", &json!({ "name": format!("B{level}"), "base": [0, 0], "handles": hs, "keep": "delete" })).unwrap();
    }
    s.execute("insert", &json!({ "name": "B7", "at": [0, 0] })).unwrap();
    let start = std::time::Instant::now();
    let r = s.execute("texts", &json!({ "limit": 1 })).unwrap();
    let n = r["count"].as_u64().unwrap();
    assert!(n > 0 && n <= 200_000, "{n}");
    assert!(start.elapsed().as_secs() < 60);
}
