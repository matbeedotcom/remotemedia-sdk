use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    let dist_dir = Path::new("dist");
    let assets_dir = dist_dir.join("assets");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let gen_path = Path::new(&out_dir).join("embedded_assets.rs");

    println!("cargo::rerun-if-changed=dist/");

    let mut out = fs::File::create(&gen_path).unwrap();

    if !dist_dir.join("index.html").exists() {
        // No build output - generate fallback
        writeln!(out, "const INDEX_HTML: &str = \"<h1>Frontend not built. Run: cd crates/ui/frontend && npm run build</h1>\";").unwrap();
        writeln!(out, "static ASSETS: &[(&str, &str, &[u8])] = &[];").unwrap();
        eprintln!("cargo::warning=Frontend not built. Run: cd crates/ui/frontend && npm run build");
        return;
    }

    // Embed index.html
    writeln!(out, "const INDEX_HTML: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/dist/index.html\"));").unwrap();

    // Find and embed all files in dist/assets/
    writeln!(out, "static ASSETS: &[(&str, &str, &[u8])] = &[").unwrap();

    if assets_dir.exists() {
        for entry in fs::read_dir(&assets_dir).unwrap() {
            let entry = entry.unwrap();
            let filename = entry.file_name().to_string_lossy().to_string();
            let content_type = if filename.ends_with(".js") {
                "application/javascript"
            } else if filename.ends_with(".css") {
                "text/css"
            } else if filename.ends_with(".wasm") {
                "application/wasm"
            } else if filename.ends_with(".svg") {
                "image/svg+xml"
            } else {
                "application/octet-stream"
            };

            // Use the full filename as the match key
            writeln!(
                out,
                "    (\"{filename}\", \"{content_type}\", include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/dist/assets/{filename}\"))),",
            ).unwrap();
        }
    }

    writeln!(out, "];").unwrap();
}
