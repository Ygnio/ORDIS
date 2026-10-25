use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};

fn read_request(s: &mut TcpStream) -> String {
    let mut data = Vec::new();
    let mut buf = [0; 1024];
    let mut total = usize::MAX;
    while data.len() < total {
        let n = s.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = data.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&data[..pos]).to_lowercase();
            let size = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|n| n.trim().parse::<usize>().ok())
                .unwrap_or(0);
            total = pos + 4 + size;
        }
    }
    String::from_utf8(data).unwrap()
}
fn event(v: Value) -> String {
    format!("data: {v}\n\n")
}
fn text(t: &str) -> String {
    event(json!({"choices":[{"delta":{"content":t}}]}))
}
fn end(f: &str) -> String {
    event(json!({"choices":[{"delta":{},"finish_reason":f}]})) + "data: [DONE]\n\n"
}
fn policy() -> Policy {
    crate::generation_policy::Config::default().select(false, false)
}
fn run(chunks: Vec<String>, p: Policy, cancel_after_text: bool, delay_ms: u64) -> ChatResponse {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let req = read_request(&mut s);
        assert!(req.contains("\"max_tokens\":32768"));
        assert!(!req.contains("n_ctx"));
        s.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
        for chunk in chunks {
            if delay_ms > 0 {
                thread::sleep(Duration::from_millis(delay_ms));
            }
            if s.write_all(chunk.as_bytes()).is_err() {
                break;
            }
            let _ = s.flush();
        }
    });
    let flag = AtomicBool::new(false);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let r = rt
        .block_on(request(
            reqwest::Client::builder().no_proxy().build().unwrap(),
            format!("http://{addr}"),
            None,
            json!({"model":"synthetic","max_tokens":32768,"stream":true}),
            &flag,
            &|k, _| {
                if k == "texto" && cancel_after_text {
                    flag.store(true, Ordering::Relaxed);
                }
            },
            p,
        ))
        .unwrap();
    server.join().unwrap();
    r
}
#[test]
fn text_and_long_responses_complete_and_specialized_limit_is_preserved() {
    for value in [
        "ok".to_string(),
        "long legitimate code line;\n".repeat(1000),
    ] {
        let r = run(vec![text(&value), end("stop")], policy(), false, 0);
        assert_eq!(r.text, value);
        assert_eq!(r.metrics.state, "completed");
        assert_eq!(r.metrics.applied_max_tokens, Some(32768));
    }
}
#[test]
fn length_retains_partial_and_does_not_fabricate_final() {
    let r = run(vec![text("partial"), end("length")], policy(), false, 0);
    assert_eq!(r.text, "partial");
    assert_eq!(r.metrics.state, "length");
    assert_eq!(r.metrics.finish_reason.as_deref(), Some("length"));
    let r = run(
        vec![
            event(json!({"choices":[{"delta":{"reasoning_content":"synthetic reasoning"}}]})),
            end("length"),
        ],
        policy(),
        false,
        0,
    );
    assert!(r.text.is_empty());
    assert_eq!(r.metrics.has_final_content, Some(false));
    assert_eq!(r.metrics.state, "length");
}
#[test]
fn reasoning_only_stop_is_not_success() {
    let r = run(
        vec![
            event(json!({"choices":[{"delta":{"reasoning_content":"thought"}}]})),
            end("stop"),
        ],
        policy(),
        false,
        0,
    );
    assert_eq!(r.metrics.state, "no_final");
}
#[test]
fn fragmented_sse_and_missing_usage_complete_without_invented_real_tokens() {
    let wire = text("Olá") + &end("stop");
    let chunks = wire
        .as_bytes()
        .chunks(1)
        .map(|b| String::from_utf8_lossy(b).to_string())
        .collect::<Vec<_>>();
    // Split ASCII framing and UTF-8 through the decoder directly, without lossy fixture encoding.
    let mut decoder = SseDecoder::default();
    let mut events = Vec::new();
    for b in wire.as_bytes() {
        events.extend(decoder.push(&[*b]));
    }
    assert!(events.iter().any(|v| v.contains("Olá")));
    let _ = chunks;
    let wire = text("ok") + &end("stop");
    let r = run(
        wire.chars().map(|c| c.to_string()).collect(),
        policy(),
        false,
        0,
    );
    assert_eq!(r.text, "ok");
    assert!(r.metrics.completion_tokens.is_none());
}
#[test]
fn disconnect_keeps_partial_and_disables_tools() {
    let r = run(vec![text("partial")], policy(), false, 0);
    assert_eq!(r.text, "partial");
    assert_eq!(r.metrics.state, "transport_error");
    assert!(!r.tools_ready());
}
#[test]
fn voluntary_cancel_keeps_partial_and_closes_before_completion() {
    let r = run(vec![text("partial"), end("stop")], policy(), true, 50);
    assert_eq!(r.metrics.state, "cancelled");
    assert_eq!(r.text, "partial");
    assert!(!r.tools_ready());
}
fn call(args: &str, id: &str) -> String {
    event(
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":id,"function":{"name":"read","arguments":args}}]}}]}),
    )
}
#[test]
fn incomplete_tools_are_never_executable_even_with_valid_json_on_length() {
    for (args, id, finish) in [
        ("{\"x\":", "id", "tool_calls"),
        ("{}", "", "tool_calls"),
        ("{}", "id", "length"),
        ("", "id", "tool_calls"),
    ] {
        let r = run(vec![call(args, id), end(finish)], policy(), false, 0);
        assert!(!r.tools_ready());
        assert!(r.tool_calls.is_empty());
    }
}
#[test]
fn complete_tool_is_executable_once_without_generated_fields() {
    let r = run(
        vec![
            call("{\"path\":\"synthetic\"}", "call-native"),
            end("tool_calls"),
        ],
        policy(),
        false,
        0,
    );
    assert!(r.tools_ready());
    assert_eq!(r.tool_calls.len(), 1);
    assert_eq!(r.tool_calls[0].id, "call-native");
}
#[test]
fn cancelled_tool_batch_cannot_execute() {
    let r = run(
        vec![text("partial"), call("{}", "id"), end("tool_calls")],
        policy(),
        true,
        50,
    );
    assert!(!r.tools_ready());
    assert!(r.tool_calls.is_empty());
}
#[test]
fn timeout_before_first_token_and_during_active_stream_are_distinct() {
    let mut p = policy();
    p.limits.first_token_seconds = 1;
    p.limits.total_seconds = 5;
    let r = run(vec![text("late"), end("stop")], p, false, 1200);
    assert_eq!(r.metrics.state, "first_token_timeout");
    let mut p = policy();
    p.limits.total_seconds = 1;
    p.limits.first_token_seconds = 1;
    p.limits.idle_seconds = 3;
    let r = run((0..20).map(|_| text("x")).collect(), p, false, 100);
    assert_eq!(r.metrics.state, "timeout");
    assert!(!r.text.is_empty());
}
#[test]
fn idle_timeout_is_not_active_generation_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        s.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{}",
                text("x")
            )
            .as_bytes(),
        )
        .unwrap();
        s.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
        let mut b = [0; 1];
        assert_eq!(s.read(&mut b).unwrap(), 0);
    });
    let mut p = policy();
    p.limits.idle_seconds = 1;
    p.limits.total_seconds = 4;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let r = rt
        .block_on(request(
            reqwest::Client::builder().no_proxy().build().unwrap(),
            format!("http://{addr}"),
            None,
            json!({"model":"synthetic"}),
            &AtomicBool::new(false),
            &|_, _| {},
            p,
        ))
        .unwrap();
    assert_eq!(r.metrics.state, "idle_timeout");
    assert_eq!(r.text, "x");
    // Hyper closes a dropped body on its runtime task. Keep that runtime driven while
    // the peer observes EOF; blocking join immediately would suspend the driver.
    rt.block_on(async { tokio::time::sleep(Duration::from_millis(80)).await });
    server.join().unwrap();
}
#[test]
fn pre_cancel_does_not_connect_and_state_allows_the_next_request() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let c = reqwest::Client::builder().no_proxy().build().unwrap();
    let r = rt
        .block_on(request(
            c,
            "http://127.0.0.1:1".into(),
            None,
            json!({"model":"synthetic"}),
            &AtomicBool::new(true),
            &|_, _| {},
            policy(),
        ))
        .unwrap();
    assert_eq!(r.metrics.state, "cancelled");
    assert_eq!(
        run(vec![text("next"), end("stop")], policy(), false, 0)
            .metrics
            .state,
        "completed"
    );
}
#[test]
fn malformed_protocol_and_duplicate_tool_ids_are_blocked() {
    let r = run(
        vec!["data: invalid\n\n".into(), end("stop")],
        policy(),
        false,
        0,
    );
    assert_eq!(r.metrics.state, "protocol_error");
    let calls = event(
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"same","function":{"name":"read","arguments":"{}"}},{"index":1,"id":"same","function":{"name":"read","arguments":"{}"}}]}}]}),
    );
    assert!(!run(vec![calls, end("tool_calls")], policy(), false, 0).tools_ready());
}
#[test]
fn backend_error_does_not_leak_response_body() {
    let r = run(
        vec![event(
            json!({"error":{"message":"PRIVATE_SYNTHETIC_NOT_EXPOSED"}}),
        )],
        policy(),
        false,
        0,
    );
    assert_eq!(r.metrics.state, "backend_error");
    assert!(r.text.is_empty());
}
#[test]
fn missing_finish_is_protocol_error_not_normal_completion() {
    let r = run(
        vec![text("partial"), "data: [DONE]\n\n".into()],
        policy(),
        false,
        0,
    );
    assert_eq!(r.metrics.state, "protocol_error");
    assert_eq!(r.text, "partial");
}

#[test]
fn malformed_tool_fields_and_oversized_names_fail_before_execution() {
    for tool in [
        json!({"index":0,"id":"id","type":"custom","function":{"name":"read","arguments":"{}"}}),
        json!({"index":0,"id":"id","function":{"name":"read","arguments":42}}),
        json!({"index":0,"id":"id","function":{"name":"x".repeat(129),"arguments":"{}"}}),
        json!({"index":0,"id":"x".repeat(1025),"function":{"name":"read","arguments":"{}"}}),
    ] {
        let r = run(
            vec![
                event(json!({"choices":[{"delta":{"tool_calls":[tool]}}]})),
                end("tool_calls"),
            ],
            policy(),
            false,
            0,
        );
        assert_eq!(r.metrics.state, "protocol_error");
        assert!(!r.tools_ready());
        assert!(r.tool_calls.is_empty());
    }
    let r = run(
        vec![
            event(
                json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"id","type":null,"function":{"name":"read","arguments":"{}"}}]}}]}),
            ),
            end("tool_calls"),
        ],
        policy(),
        false,
        0,
    );
    assert!(r.tools_ready());
}

#[test]
fn timeout_before_headers_closes_socket_and_http_errors_expose_no_body() {
    for send_error in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            read_request(&mut s);
            if send_error {
                s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 24\r\nConnection: close\r\n\r\nPRIVATE_SYNTHETIC_SECRET").unwrap();
            } else {
                s.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
                assert_eq!(s.read(&mut [0; 1]).unwrap(), 0);
            }
        });
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut p = policy();
        p.limits.first_token_seconds = 1;
        p.limits.total_seconds = 4;
        let r = rt
            .block_on(request(
                reqwest::Client::builder().no_proxy().build().unwrap(),
                format!("http://{addr}"),
                None,
                json!({"model":"synthetic"}),
                &AtomicBool::new(false),
                &|_, _| {},
                p,
            ))
            .unwrap();
        assert_eq!(
            r.metrics.state,
            if send_error {
                "backend_error"
            } else {
                "first_token_timeout"
            }
        );
        assert!(r.text.is_empty());
        assert!(r.reasoning.is_empty());
        rt.block_on(async { tokio::time::sleep(Duration::from_millis(80)).await });
        server.join().unwrap();
    }
}
