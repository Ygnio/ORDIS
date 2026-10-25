//! Explicit opt-in, one inference against the existing loopback server; no profile access.
//! Feed {body, total_seconds?, cancel_after_ms?} on stdin. Output contains metadata only.
use ordis_generation_validation::{generation_policy::Config, generation_stream};
use serde_json::{json, Value};
use std::{
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

fn main() {
    assert!(
        std::env::args().any(|a| a == "--allow-existing-localhost"),
        "Explicit opt-in required"
    );
    let mut input = String::new();
    std::io::stdin()
        .take(1048576)
        .read_to_string(&mut input)
        .unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let mut body = input["body"].clone();
    let cap = body["max_tokens"].as_u64().expect("Explicit cap required");
    assert!((1..=1024).contains(&cap));
    assert!(body.get("tools").is_none(), "No tools in live validation");
    assert!(body.get("n_ctx").is_none());
    body["stream"] = json!(true);
    body["stream_options"] = json!({"include_usage": true});
    let total = input["total_seconds"].as_u64().unwrap_or(45);
    assert!((1..=45).contains(&total));
    let mut policy = Config::default().select(false, false);
    policy.limits.total_seconds = total;
    policy.limits.first_token_seconds = total;
    policy.limits.idle_seconds = total;
    let cancel = AtomicBool::new(false);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let start = Instant::now();
    let first_final = std::cell::Cell::new(None);
    let result = rt
        .block_on(async {
            let emit = |kind: &str, _: Value| {
                if kind == "texto" && first_final.get().is_none() {
                    first_final.set(Some(start.elapsed().as_secs_f64() * 1000.0));
                }
            };
            let future = generation_stream::request(
                reqwest::Client::builder()
                    .no_proxy()
                    .retry(reqwest::retry::never())
                    .build()
                    .unwrap(),
                "http://127.0.0.1:8080".into(),
                None,
                body,
                &cancel,
                &emit,
                policy,
            );
            let timer = async {
                if let Some(ms) = input["cancel_after_ms"].as_u64() {
                    tokio::time::sleep(std::time::Duration::from_millis(ms.min(45000))).await;
                    cancel.store(true, Ordering::Relaxed);
                } else {
                    std::future::pending::<()>().await;
                }
            };
            tokio::pin!(future);
            tokio::select! {
                response = &mut future => response,
                _ = timer => future.await,
            }
        })
        .expect("Request failed before transport");
    println!(
        "{}",
        json!({"metrics":result.metrics,"first_final_ms":first_final.get(),"reasoning_present":!result.reasoning.is_empty(),"final_present":!result.text.trim().is_empty(),"synthetic_ok":result.text.trim().trim_end_matches('.')=="OK","tool_count":result.tool_calls.len()})
    );
}
