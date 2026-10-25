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
#[path = "../controle.rs"]
mod controle;
#[path = "../developer.rs"]
mod developer;
#[path = "../developer_runner.rs"]
mod developer_runner;
#[path = "../diagnostics.rs"]
mod diagnostics;
#[path = "../ferramentas.rs"]
mod ferramentas;
#[path = "../identity.rs"]
mod identity;
#[path = "../imagens.rs"]
mod imagens;
#[path = "../memory.rs"]
mod memory;
#[path = "../modelos.rs"]
mod modelos;
#[path = "../proxmox.rs"]
mod proxmox;
#[path = "../response_metrics.rs"]
mod response_metrics;
#[path = "../settings.rs"]
mod settings;
#[path = "../win.rs"]
mod win;

use std::io::Write;
use std::sync::atomic::AtomicBool;

use serde_json::Value;

struct Terminal;

impl agente::Saida for Terminal {
    fn evento(&self, tipo: &str, dados: Value) {
        let t = dados["texto"].as_str().unwrap_or("");
        match tipo {
            "developer" => {
                println!("\n[developer] {dados}");
                let _ = std::io::stdout().flush();
                if dados["state"] == "awaiting_approval" {
                    println!("[aprovação] Digite permitir para esta operação; qualquer outra resposta recusa.");
                    let _ = std::io::stdout().flush();
                    let mut answer = String::new();
                    let _ = std::io::stdin().read_line(&mut answer);
                    if let Some(id) = dados["id"].as_str() {
                        let _ = developer::decide(id, answer.trim() == "permitir");
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

fn executar(c: &controle::Caminhos, acao: &str) -> Result<(), String> {
    let imprimir = |m: &str| println!("{m}");
    match acao {
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
            let texto = std::env::args().skip(2).collect::<Vec<_>>().join(" ");
            let s = agente::nova(c)?;
            agente::responder(c, &s.id, texto, vec![], &AtomicBool::new(false), &Terminal)?;
            memory::idle_tick(c)?;
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
    let acao = std::env::args().nth(1).unwrap_or_else(|| "estado".into());
    let r = controle::Caminhos::descobrir().and_then(|c| executar(&c, &acao));
    if let Err(e) = r {
        eprintln!("ERRO: {e}");
        std::process::exit(1);
    }
}
