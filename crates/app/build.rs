// Embeds the app icon into the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets/know-giver.rc");
    println!("cargo:rerun-if-changed=assets/know-giver.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if let Err(err) =
        embed_resource::compile("assets/know-giver.rc", embed_resource::NONE).manifest_optional()
    {
        println!("cargo:warning=could not embed the app icon: {err}");
    }
}
