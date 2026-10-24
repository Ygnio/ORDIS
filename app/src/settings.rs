//! Preferências locais. O modo Desenvolvedor é apenas um plano, sem executor.
use std::fs;

use serde::{Deserialize, Serialize};

use crate::controle::Caminhos;

pub const COMFY_DEFAULT: &str = "http://127.0.0.1:8188";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Developer {
    pub workspace: String,
    pub repositories: Vec<String>,
    pub shell: String,
    pub git: String,
    pub python: String,
    pub node: String,
    pub cargo: String,
    pub timeout_seconds: u64,
    pub long_task_minutes: u64,
    pub confirmation: String,
    pub local_logs: bool,
    pub backups: bool,
    pub isolated_workspace: bool,
    pub enabled: bool,
    pub allow_read: bool,
    pub allow_write: bool,
    pub allow_execute: bool,
    pub allow_commit: bool,
    pub allow_push: bool,
}

impl Default for Developer {
    fn default() -> Self {
        Self {
            workspace: String::new(),
            repositories: vec![],
            shell: "powershell.exe".into(),
            git: String::new(),
            python: String::new(),
            node: String::new(),
            cargo: String::new(),
            timeout_seconds: 120,
            long_task_minutes: 30,
            confirmation: "always".into(),
            local_logs: true,
            backups: true,
            isolated_workspace: true,
            enabled: false,
            allow_read: false,
            allow_write: false,
            allow_execute: false,
            allow_commit: false,
            allow_push: false,
        }
    }
}

impl Developer {
    fn locked(&self) -> bool {
        !self.enabled
            && !self.allow_read
            && !self.allow_write
            && !self.allow_execute
            && !self.allow_commit
            && !self.allow_push
            && self.confirmation == "always"
            && self.isolated_workspace
            && self.backups
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub comfy_url: String,
    pub manage_comfy: bool,
    pub developer: Developer,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            comfy_url: COMFY_DEFAULT.into(),
            manage_comfy: true,
            developer: Developer::default(),
        }
    }
}

pub fn load(c: &Caminhos) -> Settings {
    let path = c.runtime.join("settings.json");
    if !path.exists() {
        return Settings::default();
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(s.trim_start_matches('\u{feff}')).ok())
        .and_then(|s| validate(s).ok())
        // Arquivo inválido não ativa gerenciamento de processos nem permissões.
        .unwrap_or_else(|| Settings {
            manage_comfy: false,
            ..Settings::default()
        })
}

fn validate(mut value: Settings) -> Result<Settings, String> {
    value.comfy_url = crate::backends::normalize_base(&value.comfy_url)?;
    if value.manage_comfy && value.comfy_url != COMFY_DEFAULT {
        return Err("O gerenciamento local só aceita http://127.0.0.1:8188. Para outro endereço, use serviço externo.".into());
    }
    let d = &value.developer;
    if !d.locked() {
        return Err("O modo Desenvolvedor ainda não pode conceder permissões.".into());
    }
    if !(1..=3600).contains(&d.timeout_seconds) || !(1..=1440).contains(&d.long_task_minutes) {
        return Err("Limites de tempo inválidos.".into());
    }
    let strings = [&d.workspace, &d.shell, &d.git, &d.python, &d.node, &d.cargo];
    if strings
        .into_iter()
        .chain(d.repositories.iter())
        .any(|s| s.len() > 4096 || s.contains(['\n', '\r', '\0']))
        || d.repositories.len() > 100
    {
        return Err("Preferências inválidas: use caminhos e nomes em uma linha.".into());
    }
    Ok(value)
}

pub fn save(c: &Caminhos, value: Settings) -> Result<(), String> {
    let value = validate(value)?;
    crate::controle::com_trava(c, false, || {
        let path = c.runtime.join("settings.json");
        let tmp = path.with_extension("tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(&value)
                .map_err(|_| "Falha ao codificar preferências.".to_string())?,
        )
        .map_err(|_| "Falha ao gravar preferências locais.".to_string())?;
        fs::rename(tmp, path).map_err(|_| "Falha ao substituir preferências locais.".to_string())
    })
}

pub fn manages_comfy(c: &Caminhos) -> bool {
    load(c).manage_comfy
}

pub fn configuration_issue(c: &Caminhos) -> Option<&'static str> {
    let path = c.runtime.join("settings.json");
    if !path.exists() {
        return None;
    }
    let valid = fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(s.trim_start_matches('\u{feff}')).ok())
        .and_then(|s| validate(s).ok())
        .is_some();
    (!valid).then_some("Preferências locais inválidas: usando valores seguros, com gerenciamento ComfyUI desativado. Os dados existentes não foram alterados.")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_installation_keeps_defaults_and_new_permissions_stay_locked() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, Settings::default());
        let mut unsafe_settings = settings;
        unsafe_settings.developer.allow_execute = true;
        assert!(validate(unsafe_settings).is_err());
    }
    #[test]
    fn external_comfy_cannot_be_managed_and_credentials_are_rejected() {
        let mut settings = Settings {
            comfy_url: "http://127.0.0.1:9191".into(),
            ..Settings::default()
        };
        assert!(validate(settings.clone()).is_err());
        settings.manage_comfy = false;
        assert!(validate(settings.clone()).is_ok());
        settings.comfy_url = "http://user:private@example.invalid".into();
        assert!(validate(settings).is_err());
    }
    #[test]
    fn preferences_round_trip_without_touching_history() {
        let root = std::env::temp_dir().join(format!(
            "ordis-settings-{}",
            crate::controle::sufixo_unico()
        ));
        let c = Caminhos {
            projeto: root.clone(),
            runtime: root.clone(),
        };
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("synthetic-history.json"), "unchanged").unwrap();
        let mut value = Settings::default();
        value.developer.workspace = "C:\\SyntheticWorkspace".into();
        value.developer.repositories = vec!["synthetic-repo".into()];
        save(&c, value.clone()).unwrap();
        assert_eq!(load(&c), value);
        assert_eq!(
            fs::read_to_string(root.join("synthetic-history.json")).unwrap(),
            "unchanged"
        );
        fs::write(root.join("settings.json"), "invalid").unwrap();
        assert!(!load(&c).manage_comfy);
        fs::remove_dir_all(root).unwrap();
    }
}
