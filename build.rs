fn main() {
    println!("cargo:rerun-if-env-changed=LUDUSAVI_VERSION");
    println!("cargo:rerun-if-env-changed=LUDUSAVI_VARIANT");
    println!("cargo:rerun-if-env-changed=LUDUSAVI_RELEASE_TAG");
    println!("cargo:rerun-if-env-changed=LUDUSAVI_UPSTREAM_BASE");

    if let Ok(tag) = std::env::var("LUDUSAVI_RELEASE_TAG") {
        println!("cargo:rustc-env=LUDUSAVI_RELEASE_TAG={tag}");
    }
    if let Ok(base) = std::env::var("LUDUSAVI_UPSTREAM_BASE") {
        println!("cargo:rustc-env=LUDUSAVI_UPSTREAM_BASE={base}");
    }

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/windows/manifest.rc");
        println!("cargo:rerun-if-changed=assets/windows/manifest.xml");

        embed_resource::compile("assets/windows/manifest.rc", embed_resource::NONE)
            .manifest_required()
            .unwrap();
    }
}
