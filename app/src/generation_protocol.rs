//! Pure protocol/state rules shared by native transport and offline tests.
use crate::response_metrics::GenerationMetrics;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub nome: String,
    pub argumentos: String,
}
#[derive(Debug)]
pub struct ChatResponse {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub metrics: GenerationMetrics,
}
impl ChatResponse {
    pub fn tools_ready(&self) -> bool {
        self.metrics.state == "completed"
            && matches!(
                self.metrics.finish_reason.as_deref(),
                Some("stop" | "tool_calls")
            )
            && !self.tool_calls.is_empty()
            && calls_valid(&self.tool_calls)
    }
}

#[derive(Default)]
pub struct SseDecoder {
    pending: Vec<u8>,
    data: Vec<String>,
    pub oversized: bool,
    pub invalid: bool,
}
impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        if self.pending.len() > 1048576 {
            self.oversized = true;
            self.pending.clear();
            return events;
        }
        while let Some(pos) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=pos).collect();
            let line = match String::from_utf8(line) {
                Ok(line) => line,
                Err(_) => {
                    self.invalid = true;
                    return events;
                }
            };
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(v) = line.strip_prefix("data:") {
                self.data.push(v.strip_prefix(' ').unwrap_or(v).to_string());
            }
            if self.data.iter().map(String::len).sum::<usize>() > 1048576 {
                self.oversized = true;
                self.data.clear();
                break;
            }
        }
        events
    }
}
fn calls_valid(calls: &[ToolCall]) -> bool {
    let mut ids = HashSet::new();
    calls.iter().all(|c| {
        !c.id.is_empty()
            && c.id.len() <= 1024
            && ids.insert(&c.id)
            && !c.nome.is_empty()
            && c.nome.len() <= 128
            && c.nome
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && c.argumentos.len() <= 1048576
            && serde_json::from_str::<Value>(&c.argumentos).is_ok_and(|v| v.is_object())
    })
}

pub fn terminal_state(
    done: bool,
    cancelled: bool,
    reason: Option<&str>,
    response: &ChatResponse,
) -> &'static str {
    if cancelled {
        return "cancelled";
    }
    if !done {
        return "transport_error";
    }
    match reason {
        Some("length") => "length",
        Some("stop" | "tool_calls") => {
            if !response.tool_calls.is_empty() && !calls_valid(&response.tool_calls) {
                return "protocol_error";
            }
            if reason == Some("tool_calls") && response.tool_calls.is_empty() {
                return "protocol_error";
            }
            if response.tool_calls.is_empty() && response.text.trim().is_empty() {
                return "no_final";
            }
            "completed"
        }
        _ => "protocol_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(text: &str) -> ChatResponse {
        ChatResponse {
            text: text.into(),
            reasoning: String::new(),
            tool_calls: vec![],
            metrics: GenerationMetrics::default(),
        }
    }
    #[test]
    fn stop_length_no_final_and_protocol_states_are_distinct() {
        let r = response("partial");
        assert_eq!(terminal_state(true, false, Some("stop"), &r), "completed");
        assert_eq!(terminal_state(true, false, Some("length"), &r), "length");
        assert_eq!(r.text, "partial");
        assert_eq!(terminal_state(true, false, None, &r), "protocol_error");
        assert_eq!(
            terminal_state(true, false, Some("stop"), &response("")),
            "no_final"
        );
        assert_eq!(
            terminal_state(true, false, Some("length"), &response("")),
            "length"
        );
    }
    #[test]
    fn user_cancel_and_transport_loss_never_grant_tool_execution() {
        let r = response("partial");
        assert_eq!(terminal_state(true, true, Some("stop"), &r), "cancelled");
        assert_eq!(
            terminal_state(false, false, Some("stop"), &r),
            "transport_error"
        );
    }
    #[test]
    fn tools_require_complete_json_objects_ids_and_names() {
        for (id, name, args) in [
            ("", "read", "{}"),
            ("id", "", "{}"),
            ("id", "read", ""),
            ("id", "read", "{\"x\":"),
            ("id", "read", "[]"),
            ("id", "read", "null"),
        ] {
            let mut r = response("");
            r.tool_calls.push(ToolCall {
                id: id.into(),
                nome: name.into(),
                argumentos: args.into(),
            });
            assert_eq!(
                terminal_state(true, false, Some("tool_calls"), &r),
                "protocol_error"
            );
            assert!(!r.tools_ready());
        }
    }
    #[test]
    fn complete_valid_tools_are_preserved_but_cannot_authorize_truncated_batches() {
        let mut r = response("");
        r.tool_calls.push(ToolCall {
            id: "native-id".into(),
            nome: "read".into(),
            argumentos: "{\"path\":\"synthetic\"}".into(),
        });
        assert_eq!(
            terminal_state(true, false, Some("tool_calls"), &r),
            "completed"
        );
        r.metrics.state = "completed".into();
        r.metrics.finish_reason = Some("tool_calls".into());
        assert!(r.tools_ready());
        for state in [
            "length",
            "timeout",
            "first_token_timeout",
            "idle_timeout",
            "cancelled",
            "transport_error",
            "protocol_error",
            "backend_error",
        ] {
            r.metrics.state = state.into();
            assert!(!r.tools_ready());
        }
        assert_eq!(r.tool_calls[0].id, "native-id");
    }
    #[test]
    fn duplicate_ids_do_not_execute_twice() {
        let mut r = response("");
        let call = ToolCall {
            id: "same".into(),
            nome: "read".into(),
            argumentos: "{}".into(),
        };
        r.tool_calls = vec![call.clone(), call];
        assert_eq!(
            terminal_state(true, false, Some("tool_calls"), &r),
            "protocol_error"
        );
    }
    #[test]
    fn sse_fragmentation_preserves_utf8_and_multiple_data_lines() {
        let wire = "data: {\"content\":\"Olá\"}\r\n\r\ndata: [DONE]\n\n";
        let mut d = SseDecoder::default();
        let mut events = vec![];
        for b in wire.as_bytes() {
            events.extend(d.push(&[*b]));
        }
        assert_eq!(events, vec!["{\"content\":\"Olá\"}", "[DONE]"]);
        assert_eq!(d.push(b"data: a\ndata: b\n\n"), vec!["a\nb"]);
    }
    #[test]
    fn oversized_unterminated_frames_are_rejected() {
        let mut d = SseDecoder::default();
        d.push(&vec![b'x'; 1048577]);
        assert!(d.oversized);
    }
    #[test]
    fn invalid_utf8_is_protocol_failure_not_replacement_text() {
        let mut d = SseDecoder::default();
        d.push(b"data: \xff\n\n");
        assert!(d.invalid);
    }
    #[test]
    fn old_history_and_new_metadata_round_trip_without_model_content() {
        let old: GenerationMetrics =
            serde_json::from_str("{\"state\":\"completed\",\"total_ms\":100}").unwrap();
        assert_eq!(old.finish_reason, None);
        let mut m = old;
        m.state = "length".into();
        m.finish_reason = Some("length".into());
        m.has_final_content = Some(true);
        let text = serde_json::to_string(&m).unwrap();
        assert!(!text.contains("reasoning_content"));
        assert_eq!(serde_json::from_str::<GenerationMetrics>(&text).unwrap(), m);
    }
}
