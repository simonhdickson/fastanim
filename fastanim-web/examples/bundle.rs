//! Writes a `.bundle` of pre-typeset text beside a script, like `fastanim bundle` but
//! without building Bevy; Trunk runs it after each build.

fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: bundle <scene.rhai>")?;
    let src = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let path = std::path::Path::new(&path);
    // Files the script loads, such as a Lottie's, are beside it.
    let dir = path.parent().unwrap_or(std::path::Path::new(""));
    fastanim_script::bake_in(&src, dir).map_err(|e| format!("{}:{e}", path.display()))?;
    let out = path.with_extension("bundle");
    std::fs::write(&out, fastanim_text::export_bundle()).map_err(|e| e.to_string())
}
