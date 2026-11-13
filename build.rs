fn main() {
    println!("cargo:rerun-if-changed=assets/arach.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/arach.rc", embed_resource::NONE).manifest_optional().unwrap();
    }
}
