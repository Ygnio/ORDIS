//! Configuração e clientes dos backends de inferência.
//!
//! O cliente llama.cpp é deliberadamente externo: este módulo não cria nem
//! registra processos, apenas faz requisições HTTP ao endpoint configurado.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::controle::Caminhos;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    Tabbyapi,
    LlamaCpp,
}

impl Default for BackendKind {
    fn default() -> Self {
        Self::Tabbyapi
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct BackendSettings {
    pub selected: BackendKind,
    pub tabby_url: String,
    pub llama_url: String,
}

impl Default for BackendSettings {
    fn default() -> Self {
        Self {
            selected: BackendKind::Tabbyapi,
            tabby_url: "http://127.0.0.1:5000".into(),
            llama_url: "http://127.0.0.1:8080".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Capabilities {
    pub streaming: bool,
    /// O servidor implementa o protocolo; o modelo/template ainda pode não
    /// produzir chamadas válidas.
    pub tools: bool,
    /// `None` significa que o servidor não anuncia essa capacidade.
    pub reasoning: Option<bool>,
    pub image_input: bool,
    pub metrics: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct BackendStatus {
    pub backend: BackendKind,
    pub connected: bool,
    pub endpoint: String,
    pub model: Option<String>,
    pub capabilities: Capabilities,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
}

fn settings_path(c: &Caminhos) -> PathBuf {
    c.runtime.join("inference/backend.json")
}
fn key_path(c: &Caminhos) -> PathBuf {
    c.runtime.join("inference/llama-api-key.dpapi")
}

pub fn settings(c: &Caminhos) -> BackendSettings {
    fs::read_to_string(settings_path(c))
        .ok()
        .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

pub fn save(c: &Caminhos, value: BackendSettings, api_key: Option<String>) -> Result<(), String> {
    let value = BackendSettings {
        tabby_url: normalize_base(&value.tabby_url)?,
        llama_url: normalize_base(&value.llama_url)?,
        ..value
    };
    let dir = c.runtime.join("inference");
    fs::create_dir_all(&dir).map_err(|e| format!("Falha ao criar configuração do backend: {e}"))?;
    let path = settings_path(c);
    let tmp = path.with_extension("tmp");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Falha ao salvar backend: {e}"))?;
    fs::rename(tmp, path).map_err(|e| format!("Falha ao salvar backend: {e}"))?;
    if let Some(key) = api_key {
        let key = key.trim();
        if key.is_empty() {
            let _ = fs::remove_file(key_path(c));
        } else {
            let encrypted = crate::proxmox::proteger(key)?;
            fs::write(key_path(c), encrypted)
                .map_err(|e| format!("Falha ao proteger a chave do backend: {e}"))?;
        }
    }
    Ok(())
}

fn llama_key(c: &Caminhos) -> Option<String> {
    fs::read(key_path(c))
        .ok()
        .and_then(|v| crate::proxmox::decifrar(&v).ok())
}

fn normalize_base(s: &str) -> Result<String, String> {
    let s = s.trim().trim_end_matches('/');
    let u =
        url::Url::parse(s).map_err(|_| "Informe uma URL válida para o servidor.".to_string())?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return Err("Use uma URL http(s) sem credenciais, query ou fragmento.".into());
    }
    Ok(s.to_string())
}

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    static TLS_PROVIDER: OnceLock<()> = OnceLock::new();
    TLS_PROVIDER.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Falha ao configurar cliente HTTP: {e}"))
}

async fn get_json(url: &str, key: Option<&str>, timeout: Duration) -> Result<Value, String> {
    let c = client(timeout)?;
    let mut req = c.get(url);
    if let Some(k) = key.filter(|k| !k.is_empty()) {
        req = req.bearer_auth(k);
    }
    let r = req.send().await.map_err(|e| {
        if e.is_timeout() {
            format!("Tempo limite ao conectar ao servidor: {e}")
        } else {
            format!("Falha de conexão: {e}")
        }
    })?;
    let status = r.status();
    if !status.is_success() {
        let body = r.text().await.unwrap_or_default();
        return Err(format!(
            "Servidor respondeu HTTP {}: {}",
            status.as_u16(),
            body.chars().take(300).collect::<String>()
        ));
    }
    r.json()
        .await
        .map_err(|e| format!("Resposta JSON inválida: {e}"))
}

fn model_data(v: &Value) -> Result<(String, bool), String> {
    let model = v["data"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or("O servidor não informou modelos carregados.")?;
    let id = model["id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("O servidor devolveu um modelo sem identificador.")?;
    let image = model["architecture"]["input_modalities"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some("image")));
    Ok((id.to_string(), image))
}

pub async fn check_llama(url: &str, key: Option<&str>) -> Result<BackendStatus, String> {
    let base = normalize_base(url)?;
    let models = get_json(&format!("{base}/v1/models"), key, Duration::from_secs(8)).await?;
    let (model, image_input) = model_data(&models)?;
    let mut metrics_request = client(Duration::from_secs(2))?.get(format!("{base}/metrics"));
    if let Some(k) = key.filter(|k| !k.is_empty()) {
        metrics_request = metrics_request.bearer_auth(k);
    }
    let metrics = metrics_request
        .send()
        .await
        .is_ok_and(|r| r.status().is_success());
    Ok(BackendStatus {
        backend: BackendKind::LlamaCpp,
        connected: true,
        endpoint: base,
        model: Some(model),
        capabilities: Capabilities {
            streaming: true,
            tools: true,
            reasoning: None,
            image_input,
            metrics,
        },
        error: None,
    })
}

async fn check_tabby(url: &str, key: &str) -> Result<String, String> {
    let base = normalize_base(url)?;
    let models = get_json(
        &format!("{base}/v1/models"),
        Some(key),
        Duration::from_secs(5),
    )
    .await?;
    model_data(&models).map(|(model, _)| model)
}

pub fn check(c: &Caminhos) -> Result<BackendStatus, String> {
    let cfg = settings(c);
    match cfg.selected {
        BackendKind::LlamaCpp => run_async(check_llama(&cfg.llama_url, llama_key(c).as_deref())),
        BackendKind::Tabbyapi => {
            let key =
                crate::controle::chave_api(c).ok_or("Chave da API do TabbyAPI não encontrada.")?;
            let base = normalize_base(&cfg.tabby_url)?;
            let model = run_async(check_tabby(&base, &key))?;
            let m = crate::modelos::conversa(c);
            Ok(BackendStatus {
                backend: BackendKind::Tabbyapi,
                connected: true,
                endpoint: base.to_string(),
                model: Some(model),
                capabilities: Capabilities {
                    streaming: true,
                    tools: !m.ferramentas.is_empty(),
                    reasoning: Some(m.raciocinio),
                    image_input: m.visao,
                    metrics: c.runtime.join("token-metrics.json").is_file(),
                },
                error: None,
            })
        }
    }
}

fn run_async<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime Tokio")
        .block_on(f)
}

pub fn active_kind(c: &Caminhos) -> BackendKind {
    settings(c).selected
}

/// Só o endpoint padrão corresponde ao serviço Tabby que o ORDIS inicia.
pub fn manages_tabby(c: &Caminhos) -> bool {
    let s = settings(c);
    let Ok(u) = url::Url::parse(&s.tabby_url) else {
        return false;
    };
    matches!(u.host_str(), Some("127.0.0.1" | "localhost"))
        && u.port_or_known_default() == Some(5000)
}

pub fn active_endpoint(c: &Caminhos) -> (String, Option<String>) {
    let s = settings(c);
    match s.selected {
        BackendKind::Tabbyapi => (s.tabby_url, crate::controle::chave_api(c)),
        BackendKind::LlamaCpp => (s.llama_url, llama_key(c)),
    }
}

pub fn selected_model(c: &Caminhos) -> Result<String, String> {
    match active_kind(c) {
        BackendKind::Tabbyapi => Ok(crate::modelos::conversa(c).modelo),
        BackendKind::LlamaCpp => {
            let (base, key) = active_endpoint(c);
            let v = run_async(get_json(
                &format!("{}/v1/models", base.trim_end_matches('/')),
                key.as_deref(),
                Duration::from_secs(8),
            ))?;
            Ok(model_data(&v)?.0)
        }
    }
}

pub fn active_capabilities(c: &Caminhos) -> Capabilities {
    check(c).map(|s| s.capabilities).unwrap_or(Capabilities {
        streaming: false,
        tools: false,
        reasoning: None,
        image_input: false,
        metrics: false,
    })
}

pub fn chat(
    c: &Caminhos,
    messages: &[Value],
    max_tokens: u32,
    tools: &[Value],
    cancel: &AtomicBool,
    emit: &dyn Fn(&str, Value),
) -> Result<ChatResponse, String> {
    let (base, key) = active_endpoint(c);
    let model = selected_model(c)?;
    let mut body =
        json!({"model": model, "messages": messages, "stream": true, "max_tokens": max_tokens});
    if !tools.is_empty() && active_capabilities(c).tools {
        body["tools"] = json!(tools);
    }
    run_async(chat_async(base, key, body, cancel, emit))
}

#[derive(Default)]
struct SseDecoder {
    pending: Vec<u8>,
    data: Vec<String>,
}

impl SseDecoder {
    fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(pos) = self.pending.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.pending.drain(..=pos).collect();
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line);
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(v) = line.strip_prefix("data:") {
                self.data.push(v.strip_prefix(' ').unwrap_or(v).to_string());
            }
        }
        events
    }
}

async fn chat_async(
    base: String,
    key: Option<String>,
    body: Value,
    cancel: &AtomicBool,
    emit: &dyn Fn(&str, Value),
) -> Result<ChatResponse, String> {
    let c = client(Duration::from_secs(900))?;
    let mut req = c
        .post(format!("{}/v1/chat/completions", base.trim_end_matches('/')).as_str())
        .json(&body);
    if let Some(k) = key.as_deref().filter(|k| !k.is_empty()) {
        req = req.bearer_auth(k);
    }
    let response = req
        .send()
        .await
        .map_err(|e| format!("Falha ao falar com o backend: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "Backend recusou o pedido (HTTP {}): {}",
            status.as_u16(),
            body.chars().take(400).collect::<String>()
        ));
    }
    let mut stream = response.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut result = ChatResponse {
        text: String::new(),
        reasoning: String::new(),
        tool_calls: vec![],
    };
    let mut ticks = tokio::time::interval(Duration::from_millis(40));
    ticks.tick().await;
    let mut finished = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let next = tokio::select! {
            biased;
            _ = ticks.tick() => {
                if cancel.load(Ordering::Relaxed) { break; }
                continue;
            },
            item = stream.next() => Some(item),
        };
        let Some(item) = next else { break };
        let Some(chunk) = item else { break };
        let chunk = chunk.map_err(|e| format!("Stream do backend interrompido: {e}"))?;
        for event in decoder.push(&chunk) {
            if event == "[DONE]" {
                finished = true;
                break;
            }
            let Ok(v) = serde_json::from_str::<Value>(&event) else {
                continue;
            };
            let delta = &v["choices"][0]["delta"];
            if let Some(s) = delta["reasoning_content"]
                .as_str()
                .filter(|s| !s.is_empty())
            {
                result.reasoning.push_str(s);
                emit("raciocinio", json!({"texto": s}));
            }
            if let Some(s) = delta["content"].as_str().filter(|s| !s.is_empty()) {
                result.text.push_str(s);
                emit("texto", json!({"texto": s}));
            }
            for call in delta["tool_calls"].as_array().into_iter().flatten() {
                let index = call["index"]
                    .as_u64()
                    .unwrap_or(result.tool_calls.len() as u64) as usize;
                while result.tool_calls.len() <= index {
                    result.tool_calls.push(ToolCall {
                        id: String::new(),
                        nome: String::new(),
                        argumentos: String::new(),
                    });
                }
                let target = &mut result.tool_calls[index];
                if let Some(id) = call["id"].as_str() {
                    target.id = id.into();
                }
                if let Some(name) = call["function"]["name"].as_str() {
                    target.nome.push_str(name);
                }
                if let Some(args) = call["function"]["arguments"].as_str() {
                    target.argumentos.push_str(args);
                }
            }
        }
        if finished {
            break;
        }
    }
    if !finished && !cancel.load(Ordering::Relaxed) {
        return Err("Stream do backend terminou sem o evento final [DONE].".into());
    }
    for (i, t) in result.tool_calls.iter_mut().enumerate() {
        if t.id.is_empty() {
            t.id = format!("call_{i}");
        }
        if t.argumentos.is_empty() {
            t.argumentos = "{}".into();
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::SystemTime;

    fn request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut body = Vec::new();
        let mut buf = [0u8; 4096];
        let mut header_end = None;
        loop {
            let n = stream.read(&mut buf).unwrap_or(0);
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n]);
            if let Some(i) = body.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = Some(i + 4);
                break;
            }
        }
        let header_end = header_end.expect("HTTP request headers");
        let header = String::from_utf8_lossy(&body[..header_end]).into_owned();
        let length = header
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length: ")
                    .and_then(|v| v.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        while body.len() < header_end + length {
            let n = stream.read(&mut buf).unwrap_or(0);
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n]);
        }
        header
    }

    fn json_response(stream: &mut TcpStream, status: &str, body: &str) {
        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn sse_decoder_handles_fragmented_lines_and_multiple_fields() {
        let mut d = SseDecoder::default();
        assert!(d.push(b"data: {\"choices\":[]\r\n").is_empty());
        assert_eq!(d.push(b"data: }\r\n\r\n"), vec!["{\"choices\":[]\n}"]);
        assert_eq!(d.push(b"data: [DONE]\n\n"), vec!["[DONE]"]);
    }

    #[test]
    fn model_metadata_does_not_assume_image_support() {
        let (id, image) = model_data(&json!({"data":[{"id":"loaded"}]})).unwrap();
        assert_eq!(id, "loaded");
        assert!(!image);
        let (_, image) = model_data(
            &json!({"data":[{"id":"vision","architecture":{"input_modalities":["text","image"]}}]}),
        )
        .unwrap();
        assert!(image);
    }

    #[test]
    fn url_validation_rejects_embedded_credentials() {
        assert!(normalize_base("http://user:secret@127.0.0.1:8080").is_err());
        assert_eq!(
            normalize_base("http://127.0.0.1:8080/").unwrap(),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn settings_default_to_managed_tabby_and_custom_tabby_is_external() {
        let root = std::env::temp_dir().join(format!(
            "ordis-backend-settings-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let c = Caminhos {
            projeto: root.clone(),
            runtime: root.clone(),
        };
        assert!(manages_tabby(&c));
        fs::create_dir_all(root.join("inference")).unwrap();
        fs::write(
            settings_path(&c),
            serde_json::to_vec(&BackendSettings {
                tabby_url: "http://127.0.0.1:5050".into(),
                ..BackendSettings::default()
            })
            .unwrap(),
        )
        .unwrap();
        assert!(!manages_tabby(&c));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mock_tabby_discovers_model_with_existing_required_auth() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let req = request(&mut s);
            assert!(req.starts_with("GET /v1/models "));
            assert!(req
                .to_ascii_lowercase()
                .contains("authorization: bearer tabby-test-key"));
            json_response(&mut s, "200 OK", r#"{"data":[{"id":"tabby-model"}]}"#);
        });
        let model = run_async(check_tabby(&format!("http://{address}"), "tabby-test-key")).unwrap();
        server.join().unwrap();
        assert_eq!(model, "tabby-model");
    }

    #[test]
    fn mock_server_discovers_model_modalities_and_optional_auth() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for (path, body) in [
                (
                    "/v1/models",
                    r#"{"data":[{"id":"gemma-gguf","architecture":{"input_modalities":["text","image"]}}]}"#,
                ),
                (
                    "/metrics",
                    "# HELP llamacpp:predicted_tokens_seconds throughput\n",
                ),
            ] {
                let (mut s, _) = listener.accept().unwrap();
                let req = request(&mut s);
                assert!(req.starts_with(&format!("GET {path} ")));
                assert!(req
                    .to_ascii_lowercase()
                    .contains("authorization: bearer local-test-key"));
                json_response(&mut s, "200 OK", body);
            }
        });
        let status = run_async(check_llama(
            &format!("http://{address}"),
            Some("local-test-key"),
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(status.model.as_deref(), Some("gemma-gguf"));
        assert!(status.capabilities.image_input);
        assert!(status.capabilities.metrics);
        assert!(status.capabilities.tools);
        assert_eq!(status.capabilities.reasoning, None);
    }

    #[test]
    fn mock_server_allows_unauthenticated_model_discovery() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for (path, body) in [
                ("/v1/models", r#"{"data":[{"id":"gguf"}]}"#),
                ("/metrics", ""),
            ] {
                let (mut s, _) = listener.accept().unwrap();
                let req = request(&mut s);
                assert!(req.starts_with(&format!("GET {path} ")));
                assert!(!req.to_ascii_lowercase().contains("authorization:"));
                json_response(&mut s, "200 OK", body);
            }
        });
        let status = run_async(check_llama(&format!("http://{address}"), None)).unwrap();
        server.join().unwrap();
        assert_eq!(status.model.as_deref(), Some("gguf"));
    }

    #[test]
    fn mock_server_streams_reasoning_text_and_tool_calls_across_chunks() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let req = request(&mut s);
            assert!(req.starts_with("POST /v1/chat/completions "));
            let events = [
                r#"data: {"choices":[{"delta":{"reasoning_content":"think"}}]}"#,
                r#"data: {"choices":[{"delta":{"content":"hello"}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-1","function":{"name":"read","arguments":"{\"path\":"}}]}}]}"#,
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"README.md\"}"}}]}}]}"#,
                "data: [DONE]",
            ];
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").unwrap();
            for event in events {
                let chunk = format!("{event}\n\n");
                write!(s, "{:X}\r\n{chunk}\r\n", chunk.len()).unwrap();
                s.flush().unwrap();
            }
            write!(s, "0\r\n\r\n").unwrap();
        });
        let cancel = AtomicBool::new(false);
        let events = std::sync::Mutex::new(Vec::new());
        let response = run_async(chat_async(
            format!("http://{address}"),
            None,
            json!({"model":"gguf","messages":[],"stream":true}),
            &cancel,
            &|kind, _| events.lock().unwrap().push(kind.to_string()),
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(response.text, "hello");
        assert_eq!(response.reasoning, "think");
        assert_eq!(response.tool_calls[0].id, "call-1");
        assert_eq!(response.tool_calls[0].nome, "read");
        assert_eq!(response.tool_calls[0].argumentos, r#"{"path":"README.md"}"#);
        assert_eq!(*events.lock().unwrap(), vec!["raciocinio", "texto"]);
    }

    #[test]
    fn cancellation_drops_an_idle_sse_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (closed_tx, closed_rx) = std::sync::mpsc::channel();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let _ = request(&mut s);
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n").unwrap();
            let chunk = "data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n";
            write!(s, "{:X}\r\n{chunk}\r\n", chunk.len()).unwrap();
            s.flush().unwrap();
            let mut b = [0; 16];
            let _ = s.read(&mut b);
            closed_tx.send(()).unwrap();
        });
        let cancel = AtomicBool::new(false);
        let response = run_async(chat_async(
            format!("http://{address}"),
            None,
            json!({"model":"gguf","messages":[],"stream":true}),
            &cancel,
            &|_, _| cancel.store(true, Ordering::Relaxed),
        ))
        .unwrap();
        assert_eq!(response.text, "x");
        closed_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn incomplete_stream_and_http_errors_are_reported() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let _ = request(&mut s);
            let body = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            s.flush().unwrap();
            let (mut s, _) = listener.accept().unwrap();
            let _ = request(&mut s);
            json_response(&mut s, "503 Service Unavailable", "server busy");
        });
        let cancel = AtomicBool::new(false);
        let incomplete = run_async(chat_async(
            format!("http://{address}"),
            None,
            json!({"model":"gguf","messages":[],"stream":true}),
            &cancel,
            &|_, _| {},
        ));
        assert!(incomplete.unwrap_err().contains("[DONE]"));
        let err = run_async(get_json(
            &format!("http://{address}/v1/models"),
            None,
            Duration::from_secs(2),
        ))
        .unwrap_err();
        assert!(err.contains("HTTP 503"));
        server.join().unwrap();
    }

    #[test]
    fn request_timeout_is_enforced() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let _ = request(&mut s);
            thread::sleep(Duration::from_millis(150));
        });
        let err = run_async(get_json(
            &format!("http://{address}/v1/models"),
            None,
            Duration::from_millis(30),
        ))
        .unwrap_err();
        assert!(
            err.to_lowercase().contains("tempo limite")
                || err.to_lowercase().contains("timeout")
                || err.to_lowercase().contains("timed out")
        );
        server.join().unwrap();
    }
}
