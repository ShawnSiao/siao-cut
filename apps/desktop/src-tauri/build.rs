fn main() {
    println!("cargo:rerun-if-env-changed=SIAOCUT_UPDATE_ENDPOINT");
    println!("cargo:rerun-if-env-changed=SIAOCUT_UPDATER_PUBKEY");
    println!("cargo:rerun-if-env-changed=SIAOCUT_UPDATER_ENABLED");
    tauri_build::build()
}
