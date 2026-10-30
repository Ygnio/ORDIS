//! `ordis-cli estado|diagnostico|iniciar|parar|perguntar "texto"|proxmox <consulta> [nó]|imagem "prompt"|benchmark [opções]`.
//! Executável de console separado, porque o ORDIS.exe é de janela e o
//! terminal não espera por ele.
#![allow(dead_code)]

#[path = "../agente.rs"]
mod agente;
#[path = "../backends.rs"]
mod backends;
#[path = "../benchmark.rs"]
mod benchmark;
#[path = "../build_info.rs"]
mod build_info;
#[path = "../cli_input.rs"]
mod cli_input;
#[path = "../configuration.rs"]
mod configuration;
#[path = "../controle.rs"]
mod controle;
#[path = "../developer.rs"]
mod developer;
#[path = "../developer_jobs.rs"]
mod developer_jobs;
#[path = "../developer_runner.rs"]
mod developer_runner;
#[path = "../diagnostics.rs"]
mod diagnostics;
#[path = "../ferramentas.rs"]
mod ferramentas;
#[path = "../generation_policy.rs"]
mod generation_policy;
#[path = "../generation_protocol.rs"]
mod generation_protocol;
#[path = "../generation_stream.rs"]
mod generation_stream;
#[path = "../identity.rs"]
mod identity;
#[path = "../imagens.rs"]
mod imagens;
#[path = "../memory.rs"]
mod memory;
#[path = "../modelos.rs"]
mod modelos;
#[path = "../official_profile.rs"]
mod official_profile;
#[path = "../proxmox.rs"]
mod proxmox;
#[path = "../response_metrics.rs"]
mod response_metrics;
#[path = "../settings.rs"]
mod settings;
#[path = "../vision.rs"]
mod vision;
#[path = "../win.rs"]
mod win;

use std::io::Write;
use std::sync::atomic::AtomicBool;

use serde_json::Value;

struct Terminal {
    input: cli_input::Input,
    legacy: bool,
}

impl agente::Saida for Terminal {
    fn evento(&self, tipo: &str, dados: Value) {
        let t = dados["texto"].as_str().unwrap_or("");
        match tipo {
            "developer" => {
                let opened = std::time::Instant::now();
                println!("\n[developer] {dados}");
                let _ = std::io::stdout().flush();
                if dados["state"] == "awaiting_approval" {
                    println!("[aprovação] Envie JSON {{\"type\":\"approval\",\"id\":\"<id>\",\"decision\":\"allow|deny\"}}; EOF recusa.");
                    if self.legacy {
                        println!("[aprovação] Compatibilidade perguntar: digite permitir para esta operação.");
                    }
                    let _ = std::io::stdout().flush();
                    if let Some(id) = dados["id"].as_str() {
                        let allow = self.input.approve(
                            id,
                            dados["timeout_seconds"].as_u64().unwrap_or(600),
                            self.legacy,
                            opened,
                        );
                        if let Err(e) = developer::decide(id, allow) {
                            eprintln!("[aprovação] {e}");
                        }
                    }
                }
            }
            "raciocinio" => eprint!("{t}"),
            "texto" => print!("{t}"),
            "ferramenta" => println!("\n[ferramenta] {} {}", dados["nome"], dados["argumentos"]),
            "resultado" => println!("[resultado] {}", t.chars().take(300).collect::<String>()),
            "status" => println!("[status] {t}"),
            _ => {}
        }
        let _ = std::io::stdout().flush();
    }
}

fn job_request(c: &controle::Caminhos) -> Result<(), String> {
    let (input, args) = cli_input::Input::start(true)?;
    let args = args.ok_or("JSON inicial ausente.")?;
    let task = format!("job-{}", controle::sufixo_unico());
    developer::begin(c, &task, "Job sintético aprovado individualmente");
    let result = developer::execute(
        c,
        &task,
        "developer_job",
        args,
        &cli_input::CANCEL,
        &|dados| {
            let opened = std::time::Instant::now();
            println!("{}", serde_json::json!({"type":"developer","data":dados}));
            let _ = std::io::stdout().flush();
            if dados["state"] == "awaiting_approval" {
                if let Some(id) = dados["id"].as_str() {
                    let cancel_path = developer_jobs::cancel_path(c, id).ok();
                    let allow = input.approve_controlled(
                        id,
                        dados["timeout_seconds"].as_u64().unwrap_or(600),
                        false,
                        opened,
                        cancel_path.as_deref(),
                    );
                    if let Err(e) = developer::decide(id, allow) {
                        eprintln!("{e}");
                    }
                }
            }
        },
    );
    developer::finish(
        c,
        &task,
        result
            .as_ref()
            .ok()
            .and_then(|v| v["state"].as_str())
            .unwrap_or("failed"),
    );
    match result {
        Ok(value) => {
            println!("{}", serde_json::json!({"type":"job_result","data":value}));
            Ok(())
        }
        Err(error) => {
            println!("{}", serde_json::json!({"type":"job_error","error":error}));
            Err(error)
        }
    }
}

fn executar(c: &controle::Caminhos, acao: &str) -> Result<(), String> {
    let imprimir = |m: &str| println!("{m}");
    match acao {
        "job" if std::env::args().nth(2).as_deref()==Some("solicitar") => return job_request(c),
        "job" => {
            use std::io::Read;
            let name=std::env::args().nth(2).ok_or("Informe solicitar, history, get, logs ou cancel.")?;
            if !["history","get","logs","cancel","fixture","projects"].contains(&name.as_str()){return Err("Ação de job não permitida.".into());}
            let mut bytes=String::new();std::io::stdin().take(65537).read_to_string(&mut bytes).map_err(|_|"Entrada inválida.")?;
            if bytes.len()>65536{return Err("Entrada excessiva.".into());}
            let args=if bytes.trim().is_empty(){serde_json::json!({})}else{serde_json::from_str(&bytes).map_err(|_|"JSON inválido.")?};
            println!("{}",developer::action(c,&format!("job_{name}"),args)?);return Ok(());
        }
        "configuracao" => {
            use std::io::Read;
            let section = std::env::args().nth(2).ok_or("Informe backend, generation ou settings.")?;
            let action = std::env::args().nth(3).unwrap_or_else(|| "load".into());
            let mut input = String::new();
            std::io::stdin().take(65537).read_to_string(&mut input)
                .map_err(|_| "Entrada inválida.")?;
            if input.len() > 65536 {
                return Err("Entrada excessiva.".into());
            }
            let value = if input.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(&input).map_err(|_| "JSON inválido.")?
            };
            println!("{}", configuration::action(c, &section, &action, value)?);
            return Ok(());
        }
        "conversar" => {
            let (input, args) = cli_input::Input::start(true)?;
            let args = args.ok_or("JSON inicial ausente.")?;
            let terminal = Terminal { input, legacy: false };
            let s = if let Some(id) = args["id"].as_str() {
                agente::abrir(c, id)?
            } else {
                agente::nova(c)?
            };
            let attachments: Vec<String> = serde_json::from_value(args["attachments"].clone())
                .map_err(|_| "Lista de anexos inválida.")?;
            agente::responder(c, &s.id, args["text"].as_str().unwrap_or("").to_string(),
                attachments, &cli_input::CANCEL, &terminal)?;
            if !cli_input::CANCEL.load(std::sync::atomic::Ordering::Relaxed) {
                memory::idle_tick(c)?;
            }
            println!("\n[sessão {}]", s.id);
            return Ok(());
        },
        "visao" => {
            use std::io::Read;
            let action=std::env::args().nth(2).ok_or("Informe load, save ou diagnostic.")?;
            let mut input=String::new();std::io::stdin().take(65537).read_to_string(&mut input).map_err(|_|"JSON inválido.")?;
            if input.len()>65536{return Err("Entrada excessiva.".into());}
            let args=if input.trim().is_empty(){serde_json::json!({})}else{serde_json::from_str(&input).map_err(|_|"JSON inválido.")?};
            println!("{}",vision::action(c,&action,args)?); return Ok(());
        },
        "indexar" => {println!("{}",memory::semantic::tick(c,&AtomicBool::new(false))?);return Ok(());},
        "desenvolvedor" => {
            use std::io::Read;
            let action = std::env::args().nth(2).ok_or("Informe load, save ou history.")?;
            let mut input = String::new();
            std::io::stdin().take(65537).read_to_string(&mut input).map_err(|_| "Entrada inválida.")?;
            if input.len() > 65536 { return Err("Entrada muito grande.".into()); }
            let args = if input.trim().is_empty() { serde_json::json!({}) } else { serde_json::from_str(&input).map_err(|_| "JSON inválido.")? };
            println!("{}", developer::action(c, &action, args)?);
            return Ok(());
        }
        "memoria" => {
            use std::io::Read;
            let action=std::env::args().nth(2).ok_or("Informe uma ação local de memória.")?;
            let mut input=String::new();std::io::stdin().take(65537).read_to_string(&mut input).map_err(|_|"Entrada JSON inválida.")?;
            if input.len()>65536{return Err("Entrada JSON muito grande.".into());}
            let args=if input.trim().is_empty(){serde_json::json!({})}else{serde_json::from_str(&input).map_err(|_|"Entrada JSON inválida.")?};
            let value=memory::action(c,&action,args)?;println!("{}",value);return Ok(());
        }
        "memoria-idle" => {memory::idle_tick(c)?;println!("{{\"ok\":true}}");return Ok(());}

        "iniciar" => controle::iniciar(c, &imprimir)?,
        "parar" => controle::parar(c, &imprimir)?,
        "estado" => {}
        "diagnostico" => {
            println!("{}", serde_json::to_string_pretty(&diagnostics::snapshot(c)).unwrap());
            return Ok(());
        }
        "perguntar" => {
            let (input, _) = cli_input::Input::start(false)?;
            let terminal = Terminal { input, legacy: true };
            let texto = std::env::args().skip(2).collect::<Vec<_>>().join(" ");
            let s = agente::nova(c)?;
            agente::responder(c, &s.id, texto, vec![], &cli_input::CANCEL, &terminal)?;
            if !cli_input::CANCEL.load(std::sync::atomic::Ordering::Relaxed) { memory::idle_tick(c)?; }
            println!("\n[sessão {}]", s.id);
            return Ok(());
        }
        "proxmox" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            let consulta = args.first().map(String::as_str).unwrap_or("resumo");
            let v = proxmox::consultar(c, consulta, args.get(1).map(String::as_str))?;
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
            return Ok(());
        }
        "benchmark" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            return benchmark::executar(c, benchmark::opcoes(&args)?);
        }
        "imagem" => {
            let prompt = std::env::args().skip(2).collect::<Vec<_>>().join(" ");
            let v = imagens::gerar(c, &serde_json::json!({"prompt": prompt}), &AtomicBool::new(false), &|s| {
                println!("[{}] {}", s["stage"].as_str().unwrap_or(""), s["message"].as_str().unwrap_or(""));
            });
            println!("{v}");
            return if v["ok"] == true { Ok(()) } else { Err(v["error"].as_str().unwrap_or("falha").into()) };
        }
        _ => {
            return Err(format!(
                "Ação desconhecida: {acao}. Use estado, diagnostico, iniciar, parar, perguntar, proxmox, imagem ou benchmark."
            ))
        }
    }
    let p = controle::painel(c);
    for s in &p.servicos {
        println!("{}: {}", s.nome, s.situacao);
    }
    println!("VRAM: {}", p.vram.as_deref().unwrap_or("indisponível"));
    Ok(())
}

fn main() {
    if matches!(
        std::env::args().nth(1).as_deref(),
        Some("--version" | "versao")
    ) {
        println!("{}", build_info::value());
        return;
    }
    if let Err(e) = cli_input::install_cancel_handler() {
        eprintln!("ERRO: {e}");
        std::process::exit(1);
    }
    let acao = std::env::args().nth(1).unwrap_or_else(|| "estado".into());
    let r = controle::Caminhos::descobrir().and_then(|c| executar(&c, &acao));
    if let Err(e) = r {
        eprintln!("ERRO: {e}");
        std::process::exit(1);
    }
}
