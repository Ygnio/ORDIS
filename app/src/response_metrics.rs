//! Per-request metrics. No prompt content, tokenizer downloads or backend process changes.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GenerationMetrics {
    pub model: String,
    pub completion_tokens: Option<u64>,
    pub estimated_tokens: Option<u64>,
    pub token_source: String,
    pub total_ms: f64,
    pub ttft_ms: Option<f64>,
    pub generation_ms: Option<f64>,
    pub generation_time_source: String,
    pub tokens_per_second: Option<f64>,
    pub rate_source: String,
    pub state: String,
}

pub struct Tracker {
    started: Instant,
    first: Option<Instant>,
    output_bytes: usize,
    model: String,
    usage_tokens: Option<u64>,
    timing_tokens: Option<u64>,
    generation_ms: Option<f64>,
    rate: Option<f64>,
}
fn positive(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite() && *x > 0.0)
}
impl Tracker {
    pub fn new(model: &str) -> Self {
        Self {
            started: Instant::now(),
            first: None,
            output_bytes: 0,
            model: model.chars().take(200).collect(),
            usage_tokens: None,
            timing_tokens: None,
            generation_ms: None,
            rate: None,
        }
    }
    pub fn observe(&mut self, v: &Value) {
        if let Some(model) = v["model"].as_str().filter(|s| !s.trim().is_empty()) {
            self.model = model.chars().take(200).collect();
        }
        if let Some(n) = v["usage"]["completion_tokens"].as_u64() {
            self.usage_tokens = Some(n);
        }
        let t = &v["timings"];
        if let Some(n) = t["predicted_n"].as_u64() {
            self.timing_tokens = Some(n);
        }
        if let Some(ms) = positive(&t["predicted_ms"]) {
            self.generation_ms = Some(ms);
        }
        if let Some(rate) = positive(&t["predicted_per_second"]) {
            self.rate = Some(rate);
        }
        let delta = &v["choices"][0]["delta"];
        let mut bytes = 0;
        for field in ["content", "reasoning_content"] {
            bytes += delta[field].as_str().map(str::len).unwrap_or(0);
        }
        let calls = delta["tool_calls"].as_array();
        for call in calls.into_iter().flatten() {
            for field in ["name", "arguments"] {
                bytes += call["function"][field].as_str().map(str::len).unwrap_or(0);
            }
        }
        if bytes > 0 || calls.is_some_and(|c| !c.is_empty()) {
            self.first.get_or_insert_with(Instant::now);
        }
        self.output_bytes = self.output_bytes.saturating_add(bytes);
    }
    pub fn snapshot(&self, state: &str) -> GenerationMetrics {
        let now = Instant::now();
        self.at(
            state,
            now.duration_since(self.started).as_secs_f64() * 1000.0,
            self.first
                .map(|t| t.duration_since(self.started).as_secs_f64() * 1000.0),
        )
    }
    fn at(&self, state: &str, total_ms: f64, ttft_ms: Option<f64>) -> GenerationMetrics {
        let cancelled = state == "cancelled";
        let tokens = self.usage_tokens.or(self.timing_tokens);
        let token_source = if self.usage_tokens.is_some() {
            "usage"
        } else if self.timing_tokens.is_some() {
            "timings"
        } else {
            "unavailable"
        };
        // Bytes/4 is a heuristic, never a tokenizer count. Cancelled responses omit it.
        let estimated = if tokens.is_none() && !cancelled && self.output_bytes > 0 {
            Some(self.output_bytes.div_ceil(4) as u64)
        } else {
            None
        };
        let local_generation = ttft_ms.map(|first| (total_ms - first).max(0.0));
        let generation_ms = self.generation_ms.or(local_generation);
        let generation_time_source = if self.generation_ms.is_some() {
            "backend"
        } else if local_generation.is_some() {
            "local_after_first"
        } else {
            "unavailable"
        };
        let (rate, rate_source) = if let Some(r) = self.rate {
            (Some(r), "backend_timings")
        } else if let (Some(n), Some(ms)) = (tokens, self.generation_ms.filter(|ms| *ms > 0.0)) {
            (Some(n as f64 * 1000.0 / ms), "backend_derived")
        } else if !cancelled {
            match (
                tokens.or(estimated),
                local_generation.filter(|ms| *ms >= 250.0),
            ) {
                // Local arrival interval excludes the first token, so also exclude it from rate.
                (Some(n), Some(ms)) if n > 1 => {
                    (Some((n - 1) as f64 * 1000.0 / ms), "local_estimate")
                }
                _ => (None, "unavailable"),
            }
        } else {
            (None, "unavailable")
        };
        GenerationMetrics {
            model: self.model.clone(),
            completion_tokens: tokens,
            estimated_tokens: estimated,
            token_source: token_source.into(),
            total_ms,
            ttft_ms,
            generation_ms,
            generation_time_source: generation_time_source.into(),
            tokens_per_second: rate,
            rate_source: rate_source.into(),
            state: state.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn completion_usage_is_not_prompt_or_total_tokens() {
        let mut t = Tracker::new("Sintético");
        t.observe(&json!({"usage":{"prompt_tokens":900,"completion_tokens":118,"total_tokens":1018},"timings":{"predicted_ms":2200.0,"predicted_per_second":53.63}}));
        let m = t.at("completed", 3300.0, Some(1100.0));
        assert_eq!(m.completion_tokens, Some(118));
        assert_eq!(m.token_source, "usage");
        assert_eq!(m.total_ms, 3300.0);
        assert_eq!(m.ttft_ms, Some(1100.0));
        assert_eq!(m.generation_ms, Some(2200.0));
        assert_eq!(m.rate_source, "backend_timings");
        assert_eq!(m.estimated_tokens, None);
    }
    #[test]
    fn no_stats_are_explicit_estimates_and_single_chunk_has_no_rate() {
        let mut t = Tracker::new("Sintético");
        t.observe(&json!({"choices":[{"delta":{"content":"Texto sintético de teste"}}]}));
        let m = t.at("streaming", 2000.0, Some(1000.0));
        assert!(m.completion_tokens.is_none());
        assert!(m.estimated_tokens.is_some());
        assert_eq!(m.rate_source, "local_estimate");
        assert!(t
            .at("completed", 1000.0, Some(1000.0))
            .tokens_per_second
            .is_none());
    }
    #[test]
    fn cancellation_only_reports_observed_data() {
        let mut t = Tracker::new("Sintético");
        assert!(t.at("cancelled", 1000.0, None).ttft_ms.is_none());
        t.observe(&json!({"choices":[{"delta":{"content":"parcial"}}]}));
        let m = t.at("cancelled", 2000.0, Some(500.0));
        assert!(m.completion_tokens.is_none());
        assert!(m.estimated_tokens.is_none());
        assert!(m.tokens_per_second.is_none());
        assert_eq!(m.state, "cancelled");
    }
    #[test]
    fn timing_fallback_and_invalid_fields() {
        let mut t = Tracker::new("Sintético");
        t.observe(&json!({"usage":{"prompt_tokens":500},"timings":{"predicted_n":20,"predicted_ms":1000,"predicted_per_second":-1}}));
        let m = t.at("completed", 1500.0, Some(500.0));
        assert_eq!(m.completion_tokens, Some(20));
        assert_eq!(m.tokens_per_second, Some(20.0));
        assert_eq!(m.token_source, "timings");
        t.observe(&json!({"usage":null,"timings":{"predicted_n":-1,"predicted_ms":0}}));
        assert_eq!(
            t.at("completed", 1500.0, Some(500.0)).completion_tokens,
            Some(20)
        );
    }
    #[test]
    fn reasoning_and_tools_trigger_ttft_without_prompt_counting() {
        let mut t = Tracker::new("Sintético");
        t.observe(&json!({"usage":{"prompt_tokens":200}}));
        assert!(t.first.is_none());
        t.observe(&json!({"choices":[{"delta":{"reasoning_content":"Teste"}}]}));
        assert!(t.first.is_some());
        let m = t.snapshot("streaming");
        let encoded = serde_json::to_string(&m).unwrap();
        assert!(!encoded.contains("Teste"));
        assert_eq!(
            serde_json::from_str::<GenerationMetrics>(&encoded).unwrap(),
            m
        );
    }
}
