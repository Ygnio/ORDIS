//! Stable production profile. Never creates a profile or rewrites environment variables.
use crate::controle::Caminhos;
use std::path::{Path, PathBuf};

fn resolve(local: &Path, configured: Option<&Path>) -> Result<Caminhos, String> {
    let project = local.join("Lucille/Interface");
    if configured.is_some_and(|p| p != project) {
        return Err(
            "ORDIS_PROJECT aponta para outro perfil; use o inicializador Lucille Interface.".into(),
        );
    }
    // Reject redirected data roots, including nested junctions.
    let actual_local = local
        .canonicalize()
        .map_err(|_| "Perfil local indisponível.")?;
    for relative in [
        "Lucille",
        "Lucille/Interface",
        "Lucille/Interface/config",
        "Lucille/Interface/runtime",
    ] {
        let p = local.join(relative);
        if p.canonicalize()
            .map_err(|_| "Perfil oficial incompleto; use o instalador.")?
            != actual_local.join(relative)
        {
            return Err("O perfil oficial não pode redirecionar para outra pasta.".into());
        }
    }
    let config = project.join("config/paths.json");
    if std::fs::symlink_metadata(&config)
        .map_err(|_| "Configuração oficial ausente.")?
        .file_type()
        .is_symlink()
    {
        return Err("Configuração oficial não pode ser link.".into());
    }
    let text = std::fs::read_to_string(config).map_err(|_| "Configuração oficial indisponível.")?;
    let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|_| "Configuração oficial inválida.")?;
    if v["runtime"].as_str() != Some("runtime") {
        return Err("O runtime oficial deve ser relativo e permanecer no perfil Interface.".into());
    }
    Ok(Caminhos {
        runtime: project.join("runtime"),
        projeto: project,
    })
}

pub fn discover() -> Result<Caminhos, String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("LOCALAPPDATA indisponível.")?;
    let configured = std::env::var_os("ORDIS_PROJECT").map(PathBuf::from);
    if cfg!(feature = "staging-interface") {
        let staging = std::env::var_os("LUCILLE_STAGING_ROOT")
            .map(PathBuf::from)
            .ok_or("Staging exige um perfil sintético explícito; não abra sobre dados oficiais.")?;
        if staging != local
            || std::fs::read_to_string(local.join("synthetic-staging.txt"))
                .ok()
                .as_deref()
                != Some("ORDIS synthetic staging\n")
        {
            return Err("Raiz de staging sintética não confirmada.".into());
        }
    }
    resolve(&local, configured.as_deref())
}

pub struct Instance(windows_sys::Win32::Foundation::HANDLE);
impl Instance {
    pub fn acquire(profile: &Path) -> Result<Option<Self>, String> {
        use sha2::{Digest, Sha256};
        use windows_sys::Win32::{
            Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS},
            System::Threading::CreateMutexW,
        };
        let id = format!(
            "{:x}",
            Sha256::digest(profile.to_string_lossy().to_lowercase().as_bytes())
        );
        let name: Vec<u16> = format!("Local\\Lucille.Interface.{id}\0")
            .encode_utf16()
            .collect();
        let h = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if h.is_null() {
            return Err("Não foi possível proteger a instância da interface.".into());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                CloseHandle(h);
            }
            return Ok(None);
        }
        Ok(Some(Self(h)))
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_profile_refuses_missing_qa_and_redirected_config() {
        let p =
            std::env::temp_dir().join(format!("lucille-official-profile-{}", std::process::id()));
        assert!(!p.exists());
        assert!(resolve(&p, None).is_err());
        let home = p.join("Lucille/Interface");
        std::fs::create_dir_all(home.join("config")).unwrap();
        std::fs::create_dir_all(home.join("runtime")).unwrap();
        let config = home.join("config/paths.json");
        std::fs::write(&config, r#"{"runtime":"runtime"}"#).unwrap();
        let c = resolve(&p, None).unwrap();
        assert_eq!(c.projeto, home);
        assert_eq!(c.runtime, home.join("runtime"));
        assert!(resolve(&p, Some(&p.join("qa-local"))).is_err());
        for wrong in ["../runtime", "C:/AI/Memory", "qa-local", ""] {
            std::fs::write(&config, serde_json::json!({"runtime":wrong}).to_string()).unwrap();
            assert!(resolve(&p, None).is_err());
        }
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn instance_lock_releases_when_owner_exits() {
        let name = std::env::temp_dir().join(format!("lucille-mutex-test-{}", std::process::id()));
        let first = Instance::acquire(&name).unwrap().unwrap();
        assert!(Instance::acquire(&name).unwrap().is_none());
        drop(first);
        assert!(Instance::acquire(&name).unwrap().is_some());
    }
}
