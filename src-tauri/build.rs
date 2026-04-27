use std::hash::{Hash, Hasher};
use std::path::PathBuf;

fn main() {
    tauri_build::build();
    // `tauri dev` kører `target/debug/hey-mikkel` uden .app — Info.plist ligger kun i bundtet produkt
    // ved `tauri build`. Uden indlejret plist ser macOS ofte TCC/ Mikrofon-listen ikke appen, og
    // NSMicrophoneUsageDescription vises ikke. Linker-sektionen __info_plist løser det for dev-binar.
    embed_macos_info_plist();
}

fn embed_macos_info_plist() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let src = manifest.join("Info.plist");
    if !src.exists() {
        return;
    }
    println!("cargo:rerun-if-changed={}", src.display());

    // ld64 får stier med mellemstof som flere token; læg kopi i temp (kort, sikkert sti).
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.to_string_lossy().hash(&mut h);
    let dest = std::env::temp_dir().join(format!("heymikkel_tcc_{:x}.plist", h.finish()));
    if std::fs::copy(&src, &dest).is_err() {
        return;
    }
    println!(
        "cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        dest.display()
    );
}
