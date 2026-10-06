//! Writes a `.bundle` of pre-typeset text beside a script, like `fastanim bundle` but
//! without building Bevy; Trunk runs it after each build.

fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: bundle <scene.rhai>")?;
    let src = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    fastanim_script::bake(&src).map_err(|e| format!("{path}:{e}"))?;
    let out = std::path::Path::new(&path).with_extension("bundle");
    std::fs::write(&out, fastanim_text::export_bundle()).map_err(|e| e.to_string())
}
