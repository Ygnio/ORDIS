//! Shared native transport, independently testable without Tauri or native C dependencies.
use crate::{
    generation_policy::{Policy, Repetition},
    response_metrics::{GenerationMetrics, Tracker},
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub use crate::generation_protocol::{ChatResponse, SseDecoder, ToolCall};
fn timeout(
    policy: &Policy,
    start: Instant,
    first: Option<Instant>,
    last: Instant,
) -> Option<&'static str> {
    crate::generation_policy::timeout_state(
        &policy.limits,
        start.elapsed(),
        first.is_some(),
        last.elapsed(),
    )
}
fn finish(
    mut result: ChatResponse,
    tracker: &Tracker,
    state: &str,
    reason: Option<String>,
    policy: &Policy,
    limit: u32,
    repetition: &Repetition,
    emit: &dyn Fn(&str, Value),
) -> ChatResponse {
    if state != "completed" {
        result.tool_calls.clear();
    }
    result.metrics = tracker.snapshot(state);
    result.metrics.backend = Some(
        if policy.profile == "tabby" {
            "tabbyapi"
        } else {
            "llama_cpp"
        }
        .into(),
    );
    result.metrics.finish_reason = reason;
    result.metrics.task_profile = Some(policy.profile.clone());
    result.metrics.applied_max_tokens = Some(limit);
    result.metrics.deadline_seconds = Some(policy.limits.total_seconds);
    result.metrics.has_final_content = Some(!result.text.trim().is_empty());
    result.metrics.repetition_warning = repetition.warning;
    result.metrics.repeated_blocks = repetition.blocks;
    emit("metricas", json!(result.metrics));
    emit("geracao_fim", json!(result.metrics));
    result
}

pub async fn request(
    client: reqwest::Client,
    base: String,
    key: Option<String>,
    mut body: Value,
    cancel: &AtomicBool,
    emit: &dyn Fn(&str, Value),
    policy: Policy,
) -> Result<ChatResponse, String> {
    let limit = match body.get("max_tokens") {
        None => {
            body["max_tokens"] = json!(policy.limits.max_tokens);
            policy.limits.max_tokens
        }
        Some(v) => v
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .map(|n| n as u32)
            .ok_or("Limite explícito de geração inválido.")?,
    };
    let mut tracker = Tracker::new(body["model"].as_str().unwrap_or("Modelo desconhecido"));
    let mut result = ChatResponse {
        text: String::new(),
        reasoning: String::new(),
        tool_calls: vec![],
        metrics: GenerationMetrics::default(),
    };
    let mut repetition = Repetition::default();
    let start = Instant::now();
    let mut first = None;
    let mut last = start;
    let mut reason: Option<String> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(40));
    let mut update = Instant::now();
    macro_rules! end {
        ($state:expr) => {
            return Ok(finish(
                result,
                &tracker,
                $state,
                reason,
                &policy,
                limit,
                &repetition,
                emit,
            ))
        };
    }
    if cancel.load(Ordering::Relaxed) {
        end!("cancelled");
    }
    let mut req = client
        .post(format!(
            "{}/v1/chat/completions",
            base.trim_end_matches('/')
        ))
        .json(&body);
    if let Some(k) = key.as_deref().filter(|k| !k.is_empty()) {
        req = req.bearer_auth(k);
    }
    let pending = req.send();
    tokio::pin!(pending);
    let response = loop {
        tokio::select! {biased;
            _=tick.tick()=>{
                if cancel.load(Ordering::Relaxed){end!("cancelled");}
                if let Some(state)=timeout(&policy,start,first,last){end!(state);}
                if update.elapsed()>=Duration::from_millis(250){emit("metricas",json!(tracker.snapshot("streaming")));update=Instant::now();}
            },
            response=&mut pending=>match response {Ok(r)=>break r,Err(_)=>{if let Some(state)=timeout(&policy,start,first,last){end!(state);}end!("transport_error");}},
        }
    };
    if !response.status().is_success() {
        end!("backend_error");
    }
    let mut stream = response.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut done = false;
    'stream: loop {
        if cancel.load(Ordering::Relaxed) {
            end!("cancelled");
        }
        if let Some(state) = timeout(&policy, start, first, last) {
            end!(state);
        }
        let item = tokio::select! {biased;
            _=tick.tick()=>{
                if update.elapsed()>=Duration::from_millis(250){emit("metricas",json!(tracker.snapshot("streaming")));update=Instant::now();}
                continue;
            },
            item=stream.next()=>item,
        };
        let Some(chunk) = item else {
            break;
        };
        let chunk = match chunk {
            Ok(v) => v,
            Err(_) => {
                if let Some(state) = timeout(&policy, start, first, last) {
                    end!(state);
                }
                end!("transport_error");
            }
        };
        let events = decoder.push(&chunk);
        if decoder.oversized || decoder.invalid {
            end!("protocol_error");
        }
        for event in events {
            if event == "[DONE]" {
                done = true;
                break 'stream;
            }
            let v: Value = match serde_json::from_str(&event) {
                Ok(v) => v,
                Err(_) => {
                    end!("protocol_error");
                }
            };
            if v.get("error").is_some() {
                end!("backend_error");
            }
            tracker.observe(&v);
            let terminal_before = reason.is_some();
            if let Some(f) = v["choices"][0]["finish_reason"].as_str() {
                if reason.as_deref().is_some_and(|old| old != f) {
                    end!("protocol_error");
                }
                reason = Some(f.to_string());
            }
            let delta = &v["choices"][0]["delta"];
            let thinking = delta["reasoning_content"].as_str().unwrap_or("");
            let text = delta["content"].as_str().unwrap_or("");
            let calls = delta["tool_calls"].as_array();
            let progress =
                !thinking.is_empty() || !text.is_empty() || calls.is_some_and(|a| !a.is_empty());
            if terminal_before && progress {
                end!("protocol_error");
            }
            if progress {
                let now = Instant::now();
                first.get_or_insert(now);
                last = now;
            }
            if !thinking.is_empty() {
                result.reasoning.push_str(thinking);
                emit("raciocinio", json!({"texto":thinking}));
            }
            if !text.is_empty() {
                result.text.push_str(text);
                emit("texto", json!({"texto":text}));
            }
            for call in calls.into_iter().flatten() {
                if call
                    .get("type")
                    .is_some_and(|v| !v.is_null() && v != "function")
                {
                    end!("protocol_error");
                }
                for value in [
                    &call["id"],
                    &call["function"]["name"],
                    &call["function"]["arguments"],
                ] {
                    if !value.is_null() && !value.is_string() {
                        end!("protocol_error");
                    }
                }
                let Some(index) = call["index"]
                    .as_u64()
                    .filter(|i| *i < 64)
                    .map(|i| i as usize)
                else {
                    end!("protocol_error");
                };
                while result.tool_calls.len() <= index {
                    result.tool_calls.push(ToolCall {
                        id: String::new(),
                        nome: String::new(),
                        argumentos: String::new(),
                    });
                }
                let target = &mut result.tool_calls[index];
                if let Some(id) = call["id"].as_str() {
                    if !target.id.is_empty() && target.id != id {
                        end!("protocol_error");
                    }
                    target.id = id.into();
                }
                if let Some(n) = call["function"]["name"].as_str() {
                    target.nome.push_str(n);
                }
                if let Some(a) = call["function"]["arguments"].as_str() {
                    target.argumentos.push_str(a);
                }
                if target.id.len() > 1024
                    || target.nome.len() > 128
                    || target.argumentos.len() > 1048576
                {
                    end!("protocol_error");
                }
            }
            if result.text.len()
                + result.reasoning.len()
                + result
                    .tool_calls
                    .iter()
                    .map(|c| c.argumentos.len())
                    .sum::<usize>()
                > 16777216
            {
                end!("protocol_error");
            }
            if policy.observe_repetition
                && repetition.observe(
                    thinking,
                    result.text.is_empty() && result.tool_calls.is_empty(),
                )
            {
                emit(
                    "geracao_aviso",
                    json!({"texto":"Possível repetição no raciocínio. A geração continua dentro dos limites configurados; você pode interromper.","repeated_blocks":repetition.blocks}),
                );
            }
        }
    }
    // A late user cancellation wins over a coincident finish and never grants tool execution.
    if cancel.load(Ordering::Relaxed) {
        end!("cancelled");
    }
    if !done {
        end!("transport_error");
    }
    let state = crate::generation_protocol::terminal_state(done, false, reason.as_deref(), &result);
    end!(state);
}

#[cfg(test)]
#[path = "generation_stream_tests.rs"]
mod tests;
