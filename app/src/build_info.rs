pub const VERSION: &str = "2026.10.30-developer-jobs.3";
pub fn value() -> serde_json::Value {
    serde_json::json!({
        "version": VERSION,
        "commit": env!("ORDIS_BUILD_COMMIT"),
        "dirty": env!("ORDIS_BUILD_DIRTY") == "true",
        "target": env!("ORDIS_BUILD_TARGET"),
        "distribution": if cfg!(feature = "official-interface") { "lucille" } else { "ordis" },
        "qa_bootstrap": false,
        "staging": cfg!(feature = "staging-interface"),
        "package_version": env!("CARGO_PKG_VERSION")
    })
}
