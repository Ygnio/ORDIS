//! Diagnóstico somente leitura, com saída reduzida e sem respostas brutas dos serviços.
use std::fs;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sysinfo::System;

use crate::controle::Caminhos;

#[derive(Clone, Debug, Serialize)]
pub struct Instructions {
    pub sse42: bool,
    pub avx: bool,
    pub avx2: bool,
    pub fma: bool,
}

impl Instructions {
    pub fn warnings(&self) -> Vec<&'static str> {
        let mut warnings = vec![];
        if !self.avx2 {
            warnings.push(
                "AVX2 não disponível. Use runtimes externos compatíveis com este processador.",
            );
        }
        if !self.fma {
            warnings.push("FMA não disponível. Não use binários que exijam FMA.");
        }
        warnings
    }
}

fn instructions() -> Instructions {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        Instructions {
            sse42: std::is_x86_feature_detected!("sse4.2"),
            avx: std::is_x86_feature_detected!("avx"),
            avx2: std::is_x86_feature_detected!("avx2"),
            fma: std::is_x86_feature_detected!("fma"),
        }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        Instructions {
            sse42: false,
            avx: false,
            avx2: false,
            fma: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gpu {
    pub name: String,
    pub total_mib: u64,
    pub used_mib: u64,
    pub free_mib: u64,
    pub driver: String,
}

fn parse_gpus(text: &str) -> Vec<Gpu> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split(',').map(str::trim).collect();
            if fields.len() != 5 {
                return None;
            }
            Some(Gpu {
                name: fields[0].into(),
                total_mib: fields[1].parse().ok()?,
                used_mib: fields[2].parse().ok()?,
                free_mib: fields[3].parse().ok()?,
                driver: fields[4].into(),
            })
        })
        .collect()
}

fn gpus() -> Vec<Gpu> {
    let Ok(mut child) = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.used,memory.free,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
    else {
        return vec![];
    };
    let limit = Instant::now() + Duration::from_secs(3);
    while Instant::now() < limit {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                return child
                    .wait_with_output()
                    .ok()
                    .map(|o| parse_gpus(&String::from_utf8_lossy(&o.stdout)))
                    .unwrap_or_default()
            }
            Ok(Some(_)) | Err(_) => return vec![],
            Ok(None) => thread::sleep(Duration::from_millis(30)),
        }
    }
    // Somente o nosso processo de consulta é encerrado no timeout.
    let _ = child.kill();
    let _ = child.wait();
    vec![]
}

pub fn vram_line() -> Option<String> {
    gpus()
        .first()
        .map(|gpu| format!("{} MiB, {} MiB", gpu.used_mib, gpu.total_mib))
}

pub fn safe_model_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 512
        && !name.contains([':', '\0', '\n', '\r'])
        && !name.starts_with(['/', '\\'])
        && name
            .split(['/', '\\'])
            .all(|part| part != ".." && !part.is_empty())
}

#[derive(Clone, Debug, Serialize)]
pub struct ComfyStatus {
    pub endpoint: String,
    pub connected: bool,
    pub managed: bool,
    pub checkpoints: Option<Vec<String>>,
    pub checkpoint_source: String,
    pub model_available: Option<bool>,
    pub workflow_configured: bool,
    pub error: Option<String>,
}

pub(crate) fn read_json(base: &str, route: &str) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(Duration::from_secs(3))
        .build();
    let response = agent
        .get(&format!("{base}{route}"))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => format!("Serviço respondeu HTTP {code}."),
            _ => "Serviço indisponível ou tempo limite de conexão.".to_string(),
        })?;
    // Não guarda nem apresenta JSON bruto, templates, caminhos ou logs.
    serde_json::from_reader(response.into_reader().take(2 * 1024 * 1024))
        .map_err(|_| "Resposta incompleta ou inválida do serviço.".to_string())
}

fn checkpoint_names(value: &Value) -> Option<Vec<String>> {
    let list = value["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"][0].as_array()?;
    Some(
        list.iter()
            .filter_map(Value::as_str)
            .filter(|s| safe_model_name(s))
            .map(str::to_string)
            .collect(),
    )
}

pub fn comfy(c: &Caminhos) -> ComfyStatus {
    let settings = crate::settings::load(c);
    let endpoint = settings.comfy_url;
    let connection = read_json(&endpoint, "/system_stats").and_then(|v| {
        if v["system"].is_object() {
            Ok(())
        } else {
            Err("ComfyUI respondeu sem informações de sistema.".into())
        }
    });
    let advertised = connection
        .as_ref()
        .ok()
        .and_then(|_| read_json(&endpoint, "/object_info/CheckpointLoaderSimple").ok())
        .and_then(|v| checkpoint_names(&v));
    let local = if settings.manage_comfy {
        Some(
            fs::read_dir(c.runtime.join("ComfyUI/models/checkpoints"))
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| {
                    e.path().is_file()
                        && e.path().extension().is_some_and(|x| {
                            ["safetensors", "ckpt", "sft"].contains(&&*x.to_string_lossy())
                        })
                })
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let source = if advertised.is_some() {
        "server"
    } else if local.is_some() {
        "local"
    } else {
        "unknown"
    };
    let mut checkpoints = advertised.or(local);
    if let Some(list) = checkpoints.as_mut() {
        list.sort();
        list.dedup();
    }
    let model = crate::modelos::imagem(c);
    let workflow_configured = !model.fluxo.is_empty();
    let model_available = if workflow_configured {
        Some(
            safe_model_name(&model.fluxo)
                && crate::modelos::pasta_fluxos(c).join(&model.fluxo).is_file(),
        )
    } else {
        checkpoints
            .as_ref()
            .map(|list| list.contains(&model.checkpoint))
    };
    ComfyStatus {
        endpoint,
        connected: connection.is_ok(),
        managed: settings.manage_comfy,
        checkpoints,
        checkpoint_source: source.into(),
        model_available,
        workflow_configured,
        error: connection.err(),
    }
}

pub fn snapshot(c: &Caminhos) -> Value {
    let mut system = System::new();
    system.refresh_memory();
    system.refresh_cpu_all();
    let instructions = instructions();
    let gpu = gpus();
    let mut warnings: Vec<String> = instructions
        .warnings()
        .into_iter()
        .map(str::to_string)
        .collect();
    if let Some(issue) = crate::settings::configuration_issue(c) {
        warnings.push(issue.to_string());
    }
    if gpu.iter().any(|g| g.free_mib < 4096) {
        warnings.push("Menos de 4 GiB de VRAM livre em uma GPU. A geração de imagens pode falhar por memória; a necessidade depende do workflow. Nenhum modelo externo será descarregado.".into());
    }
    let backend = crate::backends::status(c);
    let comfy = comfy(c);
    let proxmox = crate::proxmox::estado(c);
    json!({
        "cpu": {"name": system.cpus().first().map(|c| c.brand()), "architecture": std::env::consts::ARCH,
            "physical_cores": system.physical_core_count(), "threads": system.cpus().len(), "instructions": instructions},
        "ram": {"total_bytes": system.total_memory(), "available_bytes": system.available_memory()},
        "gpus": gpu, "gpu_detection": if gpu.is_empty() { "NVIDIA não detectada ou nvidia-smi indisponível; outras GPUs não são consultadas." } else { "nvidia-smi" },
        "windows": System::long_os_version().or_else(System::os_version),
        "webview2": tauri::webview_version().ok(),
        "backend": backend, "comfy": comfy,
        "proxmox": {"configured": proxmox.configurado, "ca_available": proxmox.ca, "connection": "Ainda não consultada; use Testar conexão em Proxmox."},
        "warnings": warnings,
        "developer": {"enabled": false, "execution": "Não implementada", "permissions": "Nenhuma permissão concedida"},
        "privacy": "Diagnóstico local; não inclui conversas, chaves, prompts, caminhos de modelos ou preferências privadas."
    })
}

use std::io::Read;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_comfy_reports_missing_checkpoint_and_is_not_registered_or_stopped() {
        use std::io::Write;
        let root =
            std::env::temp_dir().join(format!("ordis-comfy-{}", crate::controle::sufixo_unico()));
        let c = Caminhos {
            projeto: root.clone(),
            runtime: root.clone(),
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);
                assert!(request.starts_with("GET "));
                let body = if request.starts_with("GET /system_stats ") {
                    r#"{"system":{}}"#
                } else {
                    assert!(request.starts_with("GET /object_info/CheckpointLoaderSimple "));
                    r#"{"CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[[]]}}}}"#
                };
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        crate::settings::save(
            &c,
            crate::settings::Settings {
                comfy_url: format!("http://{address}"),
                manage_comfy: false,
                ..Default::default()
            },
        )
        .unwrap();
        let status = comfy(&c);
        assert!(status.connected);
        assert_eq!(status.checkpoints, Some(vec![]));
        assert_eq!(status.model_available, Some(false));
        let mut state = crate::controle::Estado::new();
        crate::controle::iniciar_servico(
            &c,
            "imagens",
            &mut state,
            &mut System::new(),
            &|_| {},
            &|| Ok(()),
        )
        .unwrap();
        assert!(state.is_empty());
        crate::controle::parar(&c, &|_| {}).unwrap();
        assert!(read_json(&format!("http://{address}"), "/system_stats").is_ok());
        server.join().unwrap();
        assert!(!root.join("state.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disconnected_comfy_keeps_model_state_unknown() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let root = std::env::temp_dir().join(format!(
            "ordis-comfy-off-{}",
            crate::controle::sufixo_unico()
        ));
        let c = Caminhos {
            projeto: root.clone(),
            runtime: root.clone(),
        };
        crate::settings::save(
            &c,
            crate::settings::Settings {
                comfy_url: format!("http://{address}"),
                manage_comfy: false,
                ..Default::default()
            },
        )
        .unwrap();
        let status = comfy(&c);
        assert!(!status.connected);
        assert_eq!(status.model_available, None);
        assert_eq!(status.checkpoints, None);
        assert!(status.error.is_some());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ivy_bridge_features_do_not_assume_avx2_or_fma() {
        let features = Instructions {
            sse42: true,
            avx: true,
            avx2: false,
            fma: false,
        };
        assert_eq!(features.warnings().len(), 2);
        assert!(!serde_json::to_value(features).unwrap()["avx2"]
            .as_bool()
            .unwrap());
    }
    #[test]
    fn gpu_memory_is_measured_and_missing_driver_is_not_invented() {
        let gpu = parse_gpus("Synthetic GPU, 12288, 9600, 2688, 000.00\ninvalid");
        assert_eq!(gpu.len(), 1);
        assert_eq!(gpu[0].free_mib, 2688);
        assert!(parse_gpus("").is_empty());
    }
    #[test]
    fn checkpoints_distinguish_empty_from_unknown_and_reject_private_paths() {
        assert_eq!(checkpoint_names(&json!({})), None);
        assert_eq!(
            checkpoint_names(
                &json!({"CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[[]]}}}})
            ),
            Some(vec![])
        );
        assert!(!safe_model_name("C:\\private\\model.safetensors"));
        assert!(!safe_model_name("../private/model.safetensors"));
        assert!(safe_model_name("sdxl/synthetic.safetensors"));
    }
    #[test]
    fn disconnected_comfy_reports_generic_error_without_server_data() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            use std::io::Write;
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            let _ = stream.read(&mut buf);
            let body = "private-token synthetic-chat C:\\private";
            write!(
                stream,
                "HTTP/1.1 503 Unavailable\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let error = read_json(&format!("http://{address}"), "/system_stats").unwrap_err();
        server.join().unwrap();
        assert_eq!(error, "Serviço respondeu HTTP 503.");
        assert!(!error.contains("private"));
    }
}
