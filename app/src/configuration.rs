//! Same persisted configuration and validation for both front ends.
use crate::controle::Caminhos;
use serde_json::{json, Value};

pub fn generation(c: &Caminhos, action: &str, value: Value) -> Result<Value, String> {
    match action {
        "load" => serde_json::to_value(crate::generation_policy::load(c)?)
            .map_err(|_| "Falha ao ler limites.".into()),
        "save" => {
            let config =
                serde_json::from_value(value).map_err(|_| "Configuração de geração inválida.")?;
            crate::generation_policy::save(c, config)?;
            Ok(json!({"saved":true}))
        }
        _ => Err("Ação de geração desconhecida.".into()),
    }
}

pub fn action(c: &Caminhos, section: &str, action: &str, value: Value) -> Result<Value, String> {
    match (section, action) {
        ("generation", _) => generation(c, action, value),
        ("backend", "load") => serde_json::to_value(crate::backends::settings(c))
            .map_err(|_| "Falha ao ler backend.".into()),
        ("backend", "save") => {
            // Keys are optional inputs, never printed. Storage remains native DPAPI.
            let config =
                serde_json::from_value(value["value"].clone()).map_err(|_| "Backend inválido.")?;
            crate::backends::save(c, config, value["api_key"].as_str().map(str::to_owned))?;
            Ok(json!({"saved":true}))
        }
        ("settings", "load") => serde_json::to_value(crate::settings::load(c))
            .map_err(|_| "Falha ao ler preferências.".into()),
        _ => Err("Seção ou ação desconhecida; identidade e memória usam memoria <ação>.".into()),
    }
}
