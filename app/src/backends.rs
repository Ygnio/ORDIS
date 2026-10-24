//! Configuração e clientes dos backends de inferência.
//!
//! O cliente llama.cpp é deliberadamente externo: este módulo não cria nem
//! registra processos, apenas faz requisições HTTP ao endpoint configurado.

use std::collections::BTreeMap;
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
    /// Suportado pelo protocolo usado pelo cliente.
    pub streaming: bool,
    /// Configuração ativa permite incluir ferramentas nas requisições; isso
    /// não prova que o modelo/template produzirá chamadas válidas.
    pub tools: bool,
    /// O protocolo do cliente aceita chamadas de ferramenta.
    pub tools_protocol: bool,
    /// `None` quando o servidor não anuncia suporte; `Some(false)` quando
    /// anunciou explicitamente que não oferece a capacidade.
    pub streaming_advertised: Option<bool>,
    pub streaming_validated: Option<bool>,
    pub tools_configured: Option<bool>,
    pub tools_advertised: Option<bool>,
    pub tools_validated: Option<bool>,
    pub reasoning_configured: Option<bool>,
    pub reasoning_validated: Option<bool>,
    pub image_input_configured: Option<bool>,
    pub image_input_advertised: Option<bool>,
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
    pub context_tokens: Option<u64>,
    pub response_max_tokens: Option<u32>,
    pub externally_managed: bool,
    pub capabilities: Capabilities,
    pub details: ServerDetails,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ServerDetails {
    pub props_available: bool,
    pub context_source: Option<String>,
    pub response_limit_source: Option<String>,
    pub sleeping: Option<bool>,
    pub flash_attention: Option<bool>,
    pub cache_type_k: Option<String>,
    pub cache_type_v: Option<String>,
    pub metrics: BTreeMap<String, f64>,
}

fn apply_props(status: &mut BackendStatus, props: &Value) {
    status.details.props_available = props.is_object();
    let defaults = &props["default_generation_settings"];
    status.context_tokens = defaults["n_ctx"].as_u64().filter(|n| *n > 0);
    if status.context_tokens.is_some() {
        status.details.context_source = Some("/props · n_ctx por slot".into());
    }
    let params = if defaults["params"].is_object() {
        &defaults["params"]
    } else {
        defaults
    };
    status.response_max_tokens = params["n_predict"]
        .as_u64()
        .or_else(|| params["max_tokens"].as_u64())
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .map(|n| n as u32);
    if status.response_max_tokens.is_some() {
        status.details.response_limit_source =
            Some("/props · limite padrão anunciado, não garantia por requisição".into());
    }
    status.details.sleeping = props["is_sleeping"].as_bool();
    status.details.flash_attention = defaults["flash_attn"].as_bool();
    let cache = |key: &str| {
        defaults[key]
            .as_str()
            .filter(|v| {
                [
                    "f32", "f16", "bf16", "q8_0", "q4_0", "q4_1", "iq4_nl", "q5_0", "q5_1",
                ]
                .contains(v)
            })
            .map(str::to_string)
    };
    status.details.cache_type_k = cache("cache_type_k");
    status.details.cache_type_v = cache("cache_type_v");
    let caps = &mut status.capabilities;
    caps.tools_advertised = props["chat_template_caps"]["supports_tool_calls"].as_bool();
    caps.tools = caps.tools_advertised != Some(false);
    caps.streaming_advertised = params["stream"].as_bool();
    // Preservar reasoning no template não prova a existência de um canal separado.
    caps.image_input_advertised = props["modalities"]["vision"]
        .as_bool()
        .or(caps.image_input_advertised);
    caps.image_input = caps.image_input_advertised.unwrap_or(false);
}

fn parse_metrics(text: &str) -> BTreeMap<String, f64> {
    let mut result = BTreeMap::new();
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        if ![
            "llamacpp:predicted_tokens_seconds",
            "llamacpp:prompt_tokens_seconds",
            "llamacpp:requests_processing",
            "llamacpp:requests_deferred",
        ]
        .contains(&name)
        {
            continue;
        }
        if let Some(value) = parts
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
        {
            result.insert(name.to_string(), value);
        }
    }
    result
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

pub(crate) fn normalize_base(s: &str) -> Result<String, String> {
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
    let mut r = req.send().await.map_err(|e| {
        if e.is_timeout() {
            format!("Tempo limite ao conectar ao servidor: {e}")
        } else {
            format!("Falha de conexão: {e}")
        }
    })?;
    let status = r.status();
    if !status.is_success() {
        return Err(format!("Servidor respondeu HTTP {}.", status.as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = r
        .chunk()
        .await
        .map_err(|_| "Resposta incompleta do servidor.".to_string())?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("Resposta de diagnóstico excede o limite.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Resposta JSON inválida.".to_string())
}

fn model_data(v: &Value) -> Result<(String, Option<bool>), String> {
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
        .map(|a| a.iter().any(|v| v.as_str() == Some("image")));
    Ok((id.to_string(), image))
}

fn public_model_id(id: &str) -> Option<String> {
    (!id.starts_with('/') && !id.contains(['\\', ':', '\n', '\r', '\0']) && id.len() <= 512)
        .then(|| id.to_string())
}

pub async fn check_llama(url: &str, key: Option<&str>) -> Result<BackendStatus, String> {
    let base = normalize_base(url)?;
    let models = get_json(&format!("{base}/v1/models"), key, Duration::from_secs(8)).await?;
    let (model, image_input_advertised) = model_data(&models)?;
    let props = get_json(&format!("{base}/props"), key, Duration::from_secs(2))
        .await
        .ok();
    let mut metrics_request = client(Duration::from_secs(2))?.get(format!("{base}/metrics"));
    if let Some(k) = key.filter(|k| !k.is_empty()) {
        metrics_request = metrics_request.bearer_auth(k);
    }
    let (metrics, measurements) = match metrics_request.send().await {
        Ok(mut response) if response.status().is_success() => {
            let mut bytes = Vec::new();
            while let Ok(Some(chunk)) = response.chunk().await {
                if bytes.len() + chunk.len() > 256 * 1024 {
                    break;
                }
                bytes.extend_from_slice(&chunk);
            }
            (true, parse_metrics(&String::from_utf8_lossy(&bytes)))
        }
        _ => (false, BTreeMap::new()),
    };
    let mut status = BackendStatus {
        backend: BackendKind::LlamaCpp,
        connected: true,
        endpoint: base,
        model: public_model_id(&model),
        context_tokens: None,
        response_max_tokens: None,
        externally_managed: true,
        capabilities: Capabilities {
            streaming: true,
            tools: true,
            tools_protocol: true,
            reasoning: None,
            streaming_advertised: None,
            streaming_validated: None,
            tools_configured: None,
            tools_advertised: None,
            tools_validated: None,
            reasoning_configured: None,
            reasoning_validated: None,
            image_input_configured: None,
            image_input_advertised,
            image_input: image_input_advertised.unwrap_or(false),
            metrics,
        },
        details: ServerDetails {
            metrics: measurements,
            ..ServerDetails::default()
        },
        error: None,
    };
    if let Some(props) = props {
        apply_props(&mut status, &props);
    }
    Ok(status)
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
                model: public_model_id(&model),
                context_tokens: manages_tabby(c).then_some(m.contexto),
                response_max_tokens: Some(m.max_tokens),
                externally_managed: !manages_tabby(c),
                details: ServerDetails {
                    context_source: Some("Configuração TabbyAPI".into()),
                    response_limit_source: Some("Configuração ORDIS/TabbyAPI".into()),
                    ..ServerDetails::default()
                },
                capabilities: Capabilities {
                    streaming: true,
                    tools: !m.ferramentas.is_empty(),
                    tools_protocol: true,
                    reasoning: None,
                    streaming_advertised: None,
                    streaming_validated: None,
                    tools_configured: Some(!m.ferramentas.is_empty()),
                    tools_advertised: None,
                    tools_validated: None,
                    reasoning_configured: Some(m.raciocinio),
                    reasoning_validated: None,
                    image_input_configured: Some(m.visao),
                    image_input_advertised: None,
                    image_input: m.visao,
                    metrics: manages_tabby(c) && c.runtime.join("token-metrics.json").is_file(),
                },
                error: None,
            })
        }
    }
}

/// Retorna o estado do backend mesmo quando está desconectado, sem incluir
/// detalhes de rede que possam conter dados privados devolvidos pelo servidor.
pub fn status(c: &Caminhos) -> BackendStatus {
    match check(c) {
        Ok(status) => status,
        Err(error) => {
            let cfg = settings(c);
            let (backend, endpoint, externally_managed) = match cfg.selected {
                BackendKind::Tabbyapi => (BackendKind::Tabbyapi, cfg.tabby_url, !manages_tabby(c)),
                BackendKind::LlamaCpp => (BackendKind::LlamaCpp, cfg.llama_url, true),
            };
            BackendStatus {
                backend,
                connected: false,
                endpoint: normalize_base(&endpoint).unwrap_or_default(),
                model: None,
                context_tokens: None,
                response_max_tokens: None,
                externally_managed,
                details: ServerDetails::default(),
                capabilities: Capabilities {
                    streaming: true,
                    tools: false,
                    tools_protocol: true,
                    reasoning: None,
                    streaming_advertised: None,
                    streaming_validated: None,
                    tools_configured: None,
                    tools_advertised: None,
                    tools_validated: None,
                    reasoning_configured: None,
                    reasoning_validated: None,
                    image_input_configured: None,
                    image_input_advertised: None,
                    image_input: false,
                    metrics: false,
                },
                error: Some(if error.starts_with("Servidor respondeu HTTP ") {
                    error
                } else if error.contains("Tempo limite") {
                    "Tempo limite ao consultar o backend.".into()
                } else {
                    "Não foi possível consultar o backend configurado.".into()
                }),
            }
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
        BackendKind::Tabbyapi if manages_tabby(c) => Ok(crate::modelos::conversa(c).modelo),
        BackendKind::Tabbyapi => available_model(c),
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

/// Sondagem leve do painel: não consulta props/métricas a cada atualização.
pub fn available_model(c: &Caminhos) -> Result<String, String> {
    let (endpoint, key) = active_endpoint(c);
    let value = run_async(get_json(
        &format!("{}/v1/models", endpoint.trim_end_matches('/')),
        key.as_deref(),
        Duration::from_secs(3),
    ))?;
    model_data(&value).map(|(model, _)| model)
}

pub fn active_capabilities(c: &Caminhos) -> Capabilities {
    check(c).map(|s| s.capabilities).unwrap_or(Capabilities {
        streaming: false,
        tools: false,
        tools_protocol: false,
        reasoning: None,
        streaming_advertised: None,
        streaming_validated: None,
        tools_configured: None,
        tools_advertised: None,
        tools_validated: None,
        reasoning_configured: None,
        reasoning_validated: None,
        image_input_configured: None,
        image_input_advertised: None,
        image_input: false,
        metrics: false,
    })
}

pub fn chat(
    c: &Caminhos,
    messages: &[Value],
    max_tokens: Option<u32>,
    tools: &[Value],
    cancel: &AtomicBool,
    emit: &dyn Fn(&str, Value),
) -> Result<ChatResponse, String> {
    let (base, key) = active_endpoint(c);
    let model = selected_model(c)?;
    let mut body = request_body(&model, messages, max_tokens);
    if !tools.is_empty() && active_capabilities(c).tools {
        body["tools"] = json!(tools);
    }
    run_async(chat_async(base, key, body, cancel, emit))
}

fn request_body(model: &str, messages: &[Value], max_tokens: Option<u32>) -> Value {
    let mut body = json!({"model": model, "messages": messages, "stream": true});
    if let Some(max_tokens) = max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    body
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
        String::from_utf8_lossy(&body).into_owned()
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
        assert_eq!(public_model_id("C:\\SyntheticPrivate\\model.gguf"), None);
        assert_eq!(public_model_id("/synthetic-private/model.gguf"), None);
        assert_eq!(public_model_id("Lucille"), Some("Lucille".into()));
        let (id, image) = model_data(&json!({"data":[{"id":"loaded"}]})).unwrap();
        assert_eq!(id, "loaded");
        assert_eq!(image, None);
        let (_, image) = model_data(
            &json!({"data":[{"id":"vision","architecture":{"input_modalities":["text","image"]}}]}),
        )
        .unwrap();
        assert_eq!(image, Some(true));
    }

    #[test]
    fn external_backend_request_does_not_inherit_tabby_response_limit() {
        let messages = vec![json!({"role":"user","content":"hello"})];
        let llama = request_body("Lucille", &messages, None);
        let tabby = request_body("tabby-model", &messages, Some(4096));
        assert!(llama.get("max_tokens").is_none());
        assert_eq!(tabby["max_tokens"], 4096);
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
    fn switching_backend_keeps_both_endpoint_configurations() {
        let root = std::env::temp_dir().join(format!(
            "ordis-backend-switch-{}-{}",
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
        let llama = BackendSettings {
            selected: BackendKind::LlamaCpp,
            tabby_url: "http://127.0.0.1:5000".into(),
            llama_url: "http://127.0.0.1:8080".into(),
        };
        save(&c, llama, None).unwrap();
        let tabby = BackendSettings {
            selected: BackendKind::Tabbyapi,
            tabby_url: "http://127.0.0.1:5050".into(),
            llama_url: "http://127.0.0.1:8090".into(),
        };
        save(&c, tabby, None).unwrap();
        let stored = settings(&c);
        assert_eq!(stored.selected, BackendKind::Tabbyapi);
        assert_eq!(stored.tabby_url, "http://127.0.0.1:5050");
        assert_eq!(stored.llama_url, "http://127.0.0.1:8090");
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
                    "/props",
                    r#"{"default_generation_settings":{"n_ctx":65536,"params":{"n_predict":4096}},"chat_template_caps":{"supports_tool_calls":true},"model_path":"C:\\private\\synthetic.gguf","chat_template":"private synthetic template"}"#,
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
        assert_eq!(status.capabilities.image_input_advertised, Some(true));
        assert!(status.capabilities.metrics);
        assert!(status.capabilities.tools);
        assert_eq!(status.capabilities.tools_advertised, Some(true));
        assert_eq!(status.capabilities.tools_validated, None);
        assert_eq!(status.capabilities.reasoning, None);
        assert_eq!(status.context_tokens, Some(65536));
        assert_eq!(status.response_max_tokens, Some(4096));
        let diagnostic = serde_json::to_string(&status).unwrap();
        assert!(!diagnostic.contains("private"));
        assert!(!diagnostic.contains("template"));
        assert!(status.externally_managed);
    }

    #[test]
    fn mock_server_allows_unauthenticated_model_discovery() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for (path, body) in [
                ("/v1/models", r#"{"data":[{"id":"gguf"}]}"#),
                ("/props", "{}"),
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
        assert_eq!(status.context_tokens, None);
        assert_eq!(status.response_max_tokens, None);
        let mut status = status;
        apply_props(
            &mut status,
            &json!({"n_ctx_train": 131072, "default_generation_settings": {"n_ctx": 0, "params": {"n_predict": -1}}}),
        );
        assert_eq!(status.context_tokens, None);
        assert_eq!(status.response_max_tokens, None);
    }

    #[test]
    fn metrics_do_not_treat_usage_or_private_labels_as_context_configuration() {
        let metrics = parse_metrics("llamacpp:predicted_tokens_seconds 21.5\nllamacpp:n_tokens_max 2048\nprivate_prompt{user=\"synthetic\"} 1\nllamacpp:requests_processing NaN");
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics["llamacpp:predicted_tokens_seconds"], 21.5);
        assert!(!serde_json::to_string(&metrics).unwrap().contains("private"));
    }

    #[test]
    fn starting_external_llama_only_checks_connection_and_does_not_register_process() {
        let root = std::env::temp_dir().join(format!(
            "ordis-llama-process-{}-{}",
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
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for _ in 0..6 {
                let (mut stream, _) = listener.accept().unwrap();
                let path = request(&mut stream);
                if path.starts_with("GET /v1/models ") {
                    json_response(&mut stream, "200 OK", r#"{"data":[{"id":"Lucille"}]}"#);
                } else if path.starts_with("GET /props ") {
                    json_response(&mut stream, "404 Not Found", "{}");
                } else {
                    assert!(path.starts_with("GET /metrics "));
                    json_response(&mut stream, "200 OK", "metrics");
                }
            }
        });
        save(
            &c,
            BackendSettings {
                selected: BackendKind::LlamaCpp,
                tabby_url: "http://127.0.0.1:5000".into(),
                llama_url: format!("http://{address}"),
            },
            None,
        )
        .unwrap();

        crate::controle::iniciar(&c, &|_| {}).unwrap();
        assert!(!c.runtime.join("state.json").exists());
        assert_eq!(check(&c).unwrap().model.as_deref(), Some("Lucille"));
        server.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mock_server_streams_reasoning_text_and_tool_calls_across_chunks() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let req = request(&mut s);
            assert!(req.starts_with("POST /v1/chat/completions "));
            assert!(req.contains("lucille_memory_data"));
            let events = [
                r#"data: {"choices":[{"delta":{"reasoning_content":"think"}}]}"#,
                r#"data: {"choices":[{"delta":{"content":"<think>texto comum</think> hello"}}]}"#,
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
            json!({"model":"gguf","messages":[crate::memory::context_with_budget(&[json!({"id":1,"data":"Contexto sintético de teste"})],1024).0.unwrap(),json!({"role":"user","content":"Teste"})],"stream":true}),
            &cancel,
            &|kind, _| events.lock().unwrap().push(kind.to_string()),
        ))
        .unwrap();
        server.join().unwrap();
        assert_eq!(response.text, "<think>texto comum</think> hello");
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
            json!({"model":"gguf","messages":[crate::memory::context_with_budget(&[json!({"id":1,"data":"Contexto sintético de teste"})],1024).0.unwrap()],"stream":true}),
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
