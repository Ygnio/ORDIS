//! Local per-request policy. Loading never writes or changes a server.
use crate::controle::Caminhos;
use serde::{Deserialize, Serialize};
use std::{fs, sync::Mutex};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_tokens: u32,
    pub total_seconds: u64,
    pub first_token_seconds: u64,
    pub idle_seconds: u64,
}
impl Limits {
    pub fn valid(&self) -> bool {
        (256..=65536).contains(&self.max_tokens)
            && (10..=3600).contains(&self.total_seconds)
            && (5..=self.total_seconds).contains(&self.first_token_seconds)
            && (5..=self.total_seconds).contains(&self.idle_seconds)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub selected_profile: String,
    pub conversation: Limits,
    pub vision: Limits,
    pub long: Limits,
    pub agent: Limits,
    pub observe_repetition: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            selected_profile: "auto".into(),
            conversation: Limits {
                max_tokens: 4096,
                total_seconds: 300,
                first_token_seconds: 120,
                idle_seconds: 90,
            },
            vision: Limits {
                max_tokens: 4096,
                total_seconds: 300,
                first_token_seconds: 120,
                idle_seconds: 90,
            },
            long: Limits {
                max_tokens: 16384,
                total_seconds: 900,
                first_token_seconds: 300,
                idle_seconds: 180,
            },
            agent: Limits {
                max_tokens: 8192,
                total_seconds: 600,
                first_token_seconds: 180,
                idle_seconds: 120,
            },
            observe_repetition: true,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Policy {
    pub profile: String,
    pub limits: Limits,
    pub observe_repetition: bool,
}
impl Config {
    pub fn valid(&self) -> bool {
        ["auto", "conversation", "vision", "long", "agent"]
            .contains(&self.selected_profile.as_str())
            && [&self.conversation, &self.vision, &self.long, &self.agent]
                .iter()
                .all(|v| v.valid())
    }
    pub fn select(&self, images: bool, agent: bool) -> Policy {
        let profile = if self.selected_profile != "auto" {
            self.selected_profile.as_str()
        } else if agent {
            "agent"
        } else if images {
            "vision"
        } else {
            "conversation"
        };
        let limits = match profile {
            "vision" => &self.vision,
            "long" => &self.long,
            "agent" => &self.agent,
            _ => &self.conversation,
        };
        Policy {
            profile: profile.into(),
            limits: limits.clone(),
            observe_repetition: self.observe_repetition,
        }
    }
}
pub fn load(c: &Caminhos) -> Result<Config, String> {
    let path = c.runtime.join("inference/generation.json");
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|_| "Não foi possível ler os limites locais de geração.")?;
    let value:Config=serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_|"Limites locais de geração inválidos. Revise a configuração; o arquivo não foi alterado.")?;
    if !value.valid() {
        return Err("Limites locais fora dos intervalos permitidos; revise a configuração.".into());
    }
    Ok(value)
}
pub fn save(c: &Caminhos, value: Config) -> Result<(), String> {
    static LOCK: Mutex<()> = Mutex::new(());
    if !value.valid() {
        return Err("Limites inválidos: tokens 256–65536, total 10–3600 s, primeiro token/inatividade 5 s até o prazo total.".into());
    }
    let _lock = LOCK.lock().map_err(|_| "Configuração ocupada.")?;
    let path = c.runtime.join("inference/generation.json");
    fs::create_dir_all(path.parent().unwrap())
        .map_err(|_| "Falha ao criar pasta da configuração.")?;
    let tmp = path.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(&value).map_err(|_| "Configuração inválida.")?,
    )
    .map_err(|_| "Falha ao salvar limites.")?;
    fs::rename(tmp, path).map_err(|_| "Falha ao substituir configuração dos limites.".into())
}

pub fn timeout_state(
    limits: &Limits,
    elapsed: std::time::Duration,
    first_seen: bool,
    idle: std::time::Duration,
) -> Option<&'static str> {
    if elapsed >= std::time::Duration::from_secs(limits.total_seconds) {
        Some("timeout")
    } else if !first_seen && elapsed >= std::time::Duration::from_secs(limits.first_token_seconds) {
        Some("first_token_timeout")
    } else if first_seen && idle >= std::time::Duration::from_secs(limits.idle_seconds) {
        Some("idle_timeout")
    } else {
        None
    }
}

/// Bounded, conservative observation of *reasoning only*. Never cancels.
#[derive(Default)]
pub struct Repetition {
    tail: String,
    pending: usize,
    pub blocks: u32,
    pub warning: bool,
    structured: bool,
}
impl Repetition {
    pub fn observe(&mut self, text: &str, allowed: bool) -> bool {
        if !allowed || self.warning {
            return false;
        }
        self.tail.push_str(text);
        self.pending += text.len();
        if [
            "```", "{", "[", "|", "\"", ">", "\n-", "\n*", "\n1.", "ERROR", "INFO", "WARN",
        ]
        .iter()
        .any(|p| self.tail.contains(p))
        {
            self.structured = true;
        }
        if self.structured {
            self.tail.clear();
            return false;
        }
        if self.tail.len() > 16384 {
            let mut start = self.tail.len() - 16384;
            while !self.tail.is_char_boundary(start) {
                start += 1;
            }
            self.tail.drain(..start);
        }
        if self.pending < 512 || self.tail.len() < 4096 {
            return false;
        }
        self.pending = 0;
        // Structured/quoted content is intentionally excluded, including code/JSON/logs/tables/lists.
        for width in [256, 512, 1024] {
            if !self.tail.is_char_boundary(self.tail.len() - width) {
                continue;
            }
            let suffix = &self.tail[self.tail.len() - width..];
            let repeats = self.tail.matches(suffix).count();
            self.blocks = self.blocks.max(repeats as u32);
            if repeats >= 8 {
                self.warning = true;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn total_first_token_and_idle_clocks_are_independent() {
        use std::time::Duration as D;
        let p = Config::default().conversation;
        assert_eq!(
            timeout_state(&p, D::from_secs(120), false, D::ZERO),
            Some("first_token_timeout")
        );
        assert_eq!(
            timeout_state(&p, D::from_secs(120), true, D::from_secs(5)),
            None
        );
        assert_eq!(
            timeout_state(&p, D::from_secs(200), true, D::from_secs(90)),
            Some("idle_timeout")
        );
        assert_eq!(
            timeout_state(&p, D::from_secs(300), true, D::ZERO),
            Some("timeout")
        );
    }
    #[test]
    fn profiles_preserve_long_tasks_and_explicit_selection() {
        let mut c = Config::default();
        assert!(c.valid());
        assert_eq!(c.select(false, false).limits.max_tokens, 4096);
        assert_eq!(c.select(true, false).profile, "vision");
        assert_eq!(c.select(false, true).limits.max_tokens, 8192);
        c.selected_profile = "long".into();
        assert_eq!(c.select(true, true).limits.max_tokens, 16384);
        c.long.max_tokens = 20000;
        assert_eq!(c.select(false, false).limits.max_tokens, 20000);
        c.long.idle_seconds = 0;
        assert!(!c.valid());
    }
    #[test]
    fn repetition_is_observation_only_and_excludes_structure() {
        let prose="Verifico novamente a mesma hipótese sem qualquer evidência nova para decidir a identificação. ".repeat(140);
        let mut r = Repetition::default();
        r.observe(&prose, true);
        assert!(r.warning);
        for marker in [
            "```rust\n",
            "{\"a\":1}",
            "[1,2]",
            "\n- item\n",
            "\n1. item\n",
            "|table|",
            "\"quote\"",
            "INFO log\n",
        ] {
            let mut r = Repetition::default();
            r.observe(&(marker.to_string() + &prose), true);
            assert!(!r.warning);
        }
        let mut r = Repetition::default();
        r.observe(&prose, false);
        assert!(!r.warning);
        let mut r = Repetition::default();
        r.observe("Again again again", true);
        assert!(!r.warning);
        let mut r = Repetition::default();
        r.observe(&("```rust\n".to_string() + &prose.repeat(5)), true);
        assert!(!r.warning);
    }
    #[test]
    fn config_round_trip_is_local_and_invalid_files_are_not_replaced() {
        let root = std::env::temp_dir().join(format!("generation-policy-{}", std::process::id()));
        let c = Caminhos {
            projeto: root.clone(),
            runtime: root.clone(),
        };
        assert_eq!(load(&c).unwrap(), Config::default());
        save(&c, Config::default()).unwrap();
        assert_eq!(load(&c).unwrap(), Config::default());
        let path = c.runtime.join("inference/generation.json");
        fs::write(&path, "invalid").unwrap();
        assert!(load(&c).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "invalid");
        fs::remove_file(path).unwrap();
        fs::remove_dir(root.join("inference")).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
