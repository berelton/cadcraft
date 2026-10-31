//! PLOT writes the format its path names (NSAI-8096 A5): `plot` to `x.svg` used to write a PDF
//! named x.svg.

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use serde_json::json;

#[test]
fn plot_follows_the_path_extension() {
    set_io(IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
    });
    let dir = std::env::temp_dir().join(format!("cadcraft-plot-format-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::new();
    s.execute("new", &json!({ "metric": true })).unwrap();
    s.execute("line", &json!({ "points": [[0, 0], [100, 50]] })).unwrap();
    for (ext, magic) in [("svg", &b"<"[..]), ("SVG", b"<"), ("png", b"\x89PNG"), ("pdf", b"%PDF"), ("plt", b"%PDF")] {
        let path = dir.join(format!("out.{ext}"));
        s.execute("plot", &json!({ "path": path.to_string_lossy(), "layout": "Model" })).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let start = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_string();
        let ok = if magic == b"<" { start.trim_start().starts_with('<') && start.contains("svg") } else { bytes.starts_with(magic) };
        assert!(ok, "{ext}: {start:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
