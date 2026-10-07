fn main() {
    println!("cargo:rerun-if-changed=src/macos.c");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new().file("src/macos.c").flag("-mmacosx-version-min=14.0").compile("cym_macos");
    }
}
