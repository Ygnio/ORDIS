//! Compiles the actual production modules. HTTP-only loopback test harness; no Tauri/C build.
pub mod controle {
    pub struct Caminhos {
        pub projeto: std::path::PathBuf,
        pub runtime: std::path::PathBuf,
    }
}
#[path = "../../../app/src/generation_policy.rs"]
pub mod generation_policy;
#[path = "../../../app/src/generation_protocol.rs"]
pub mod generation_protocol;
#[path = "../../../app/src/generation_stream.rs"]
#[cfg(feature = "transport")]
pub mod generation_stream;
#[path = "../../../app/src/response_metrics.rs"]
pub mod response_metrics;
