//! Structured jobs are restricted to an immutable catalogue of reviewed synthetic projects.
//! This is an execution policy, not an OS sandbox for arbitrary projects.
use crate::controle::Caminhos;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Tool {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub enabled: bool,
    pub rust_bin: String,
    pub gnu_bin: String,
    pub msvc_bin: String,
    pub msvc_environment: BTreeMap<String, String>,
    pub node: String,
    pub python: String,
    pub target: String,
    pub seconds: u64,
    pub subprocess_seconds: u64,
    pub memory_mib: usize,
    pub cpu_percent: u32,
    pub processes: u32,
    pub storage_mib: u64,
    pub tools: BTreeMap<String, Tool>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            rust_bin: String::new(),
            gnu_bin: String::new(),
            msvc_bin: String::new(),
            msvc_environment: BTreeMap::new(),
            node: String::new(),
            python: String::new(),
            target: "x86_64-pc-windows-gnu".into(),
            seconds: 60,
            subprocess_seconds: 60,
            memory_mib: 512,
            cpu_percent: 50,
            processes: 8,
            storage_mib: 128,
            tools: BTreeMap::new(),
        }
    }
}
pub fn validate(cfg: &Config) -> Result<(), String> {
    if cfg.msvc_environment.keys().any(|k| {
        ![
            "INCLUDE",
            "LIB",
            "LIBPATH",
            "WindowsSdkDir",
            "VCToolsInstallDir",
        ]
        .contains(&k.as_str())
    }) {
        return Err("Ambiente MSVC contém chave não autorizada.".into());
    }
    if !(1..=600).contains(&cfg.seconds)
        || !(1..=cfg.seconds).contains(&cfg.subprocess_seconds)
        || !(128..=2048).contains(&cfg.memory_mib)
        || !(1..=80).contains(&cfg.cpu_percent)
        || !(2..=16).contains(&cfg.processes)
        || !(16..=512).contains(&cfg.storage_mib)
        || !["x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"].contains(&cfg.target.as_str())
    {
        return Err("Limites ou target de job inválidos.".into());
    }
    Ok(())
}
pub fn file_hash(path: &Path) -> Result<String, String> {
    let mut f = File::open(path).map_err(|_| "Ferramenta/fonte indisponível.")?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|_| "Falha ao verificar hash.")?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn tool(path: PathBuf, expected: &str, workspace: &Path) -> Result<Tool, String> {
    if !path.is_absolute()
        || path
            .file_name()
            .and_then(|p| p.to_str())
            .is_none_or(|n| !n.eq_ignore_ascii_case(expected))
    {
        return Err("Selecione executável nativo absoluto com o nome esperado.".into());
    }
    let actual = path
        .canonicalize()
        .map_err(|_| "Executável ausente; instalação automática proibida.")?;
    if actual.starts_with(workspace) {
        return Err("Ferramentas devem permanecer fora do workspace editável.".into());
    }
    let _guard = crate::developer::guard(&path, false)?;
    let sha256 = file_hash(&actual)?;
    let proxy = actual.parent().unwrap().join("rustup.exe");
    if proxy.is_file() && file_hash(&proxy)? == sha256 {
        return Err("Proxy rustup recusado; selecione o executável nativo instalado.".into());
    }
    Ok(Tool {
        path: local_path(&actual)?,
        sha256,
    })
}
fn local_path(path: &Path) -> Result<PathBuf, String> {
    use std::path::{Component, Prefix};
    if !matches!(path.components().next(),Some(Component::Prefix(p)) if matches!(p.kind(),Prefix::Disk(_)|Prefix::VerbatimDisk(_)))
    {
        return Err("Jobs exigem caminhos em disco local, sem UNC/dispositivos.".into());
    }
    let text = path.to_string_lossy();
    Ok(PathBuf::from(text.strip_prefix("\\\\?\\").unwrap_or(&text)))
}
/// Only the human configuration action calls this; loads never refresh pinned hashes.
pub fn configure(cfg: &mut Config, workspace: &Path) -> Result<(), String> {
    validate(cfg)?;
    let mut tools = BTreeMap::new();
    for (key, path, name) in [
        ("python", cfg.python.clone(), "python.exe"),
        ("node", cfg.node.clone(), "node.exe"),
    ] {
        if !path.is_empty() {
            tools.insert(key.into(), tool(PathBuf::from(path), name, workspace)?);
        }
    }
    if !cfg.rust_bin.is_empty() {
        let bin = PathBuf::from(&cfg.rust_bin);
        for name in [
            "cargo",
            "rustc",
            "rustdoc",
            "rustfmt",
            "cargo-fmt",
            "cargo-clippy",
            "clippy-driver",
        ] {
            let filename = format!("{name}.exe");
            let path = bin.join(&filename);
            if path.is_file() {
                tools.insert(name.into(), tool(path, &filename, workspace)?);
            }
        }
    }
    if !cfg.gnu_bin.is_empty() {
        let bin = PathBuf::from(&cfg.gnu_bin);
        for name in ["gcc", "ar", "as", "dlltool", "ld"] {
            let filename = format!("{name}.exe");
            tools.insert(
                name.into(),
                tool(bin.join(&filename), &filename, workspace)?,
            );
        }
    }
    cfg.tools = tools;
    if !cfg.msvc_bin.is_empty() {
        for name in ["cl", "link"] {
            let filename = format!("{name}.exe");
            cfg.tools.insert(
                name.into(),
                tool(
                    PathBuf::from(&cfg.msvc_bin).join(&filename),
                    &filename,
                    workspace,
                )?,
            );
        }
        cfg.msvc_environment.clear();
        for key in [
            "INCLUDE",
            "LIB",
            "LIBPATH",
            "WindowsSdkDir",
            "VCToolsInstallDir",
        ] {
            if let Ok(value) = std::env::var(key) {
                cfg.msvc_environment.insert(key.into(), value);
            }
        }
    }
    Ok(())
}

pub const OPERATIONS: &[&str] = &[
    "rust_check",
    "rust_test",
    "rust_fmt",
    "rust_clippy",
    "node_version",
    "node_syntax",
    "node_test",
    "python_syntax",
    "python_test",
];
const CASES: &[&str] = &[
    "pass",
    "compile_error",
    "test_fail",
    "wait",
    "output",
    "child",
    "unformatted",
];
fn files(language: &str, case: &str) -> Result<BTreeMap<String, String>, String> {
    if !CASES.contains(&case) {
        return Err("Caso sintético desconhecido.".into());
    }
    let mut f = BTreeMap::new();
    match language {
        "rust" => {
            if case == "child" {
                return Err("Caso filho disponível em Python/Node.".into());
            }
            f.insert(
                "Cargo.toml".into(),
                "[package]\nname = \"ordis_synthetic\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
                    .into(),
            );
            f.insert(
                "Cargo.lock".into(),
                "version = 4\n\n[[package]]\nname = \"ordis_synthetic\"\nversion = \"0.1.0\"\n"
                    .into(),
            );
            let body = match case {
                "compile_error" => "    missing_synthetic_symbol\n",
                _ => "    a + b\n",
            };
            let test=match case {"test_fail"=>"        assert_eq!(add(2, 3), 99);\n","wait"=>"        std::thread::sleep(std::time::Duration::from_secs(30));\n","output"=>"        for _ in 0..100000 {\n            println!(\"synthetic output {}\", \"x\".repeat(512));\n        }\n",_=>"        assert_eq!(add(2, 3), 5);\n"};
            let source = if case == "unformatted" {
                "pub fn add(a:i32,b:i32)->i32{a+b}\n".into()
            } else {
                format!("pub fn add(a: i32, b: i32) -> i32 {{\n{body}}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn synthetic_test() {{\n{test}    }}\n}}\n")
            };
            f.insert("src/lib.rs".into(), source);
        }
        "node" => {
            f.insert(
                "app.cjs".into(),
                if case == "compile_error" {
                    "function broken( {\n"
                } else {
                    "exports.add = (a, b) => a + b;\n"
                }
                .into(),
            );
            let source=match case {
                "wait"=>"const { test } = require('node:test'); test('wait', async () => { await new Promise(r => setTimeout(r, 30000)); });\n".into(),
                "output"=>"const { test } = require('node:test'); test('output', async () => { await new Promise(() => setInterval(() => process.stdout.write('x'.repeat(8192)), 1)); });\n".into(),
                "child"=>"const { spawn } = require('node:child_process'); const p = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], {stdio:'ignore'}); console.log('CHILD_PID='+p.pid); setInterval(()=>{},1000);\n".into(),
                _=>format!("const {{ test }} = require('node:test'); const assert = require('node:assert/strict'); const {{ add }} = require('./app.cjs'); test('synthetic add', () => assert.equal(add(2,3),{}));\n",if case=="test_fail"{99}else{5})
            };
            f.insert("test_app.cjs".into(), source);
        }
        "python" => {
            f.insert(
                "app.py".into(),
                if case == "compile_error" {
                    "def broken(:\n"
                } else {
                    "def add(a, b):\n    return a + b\n"
                }
                .into(),
            );
            let source=match case {
                "wait"=>"import unittest\nimport time\nclass Synthetic(unittest.TestCase):\n    def test_wait(self):\n        time.sleep(30)\n".into(),
                "output"=>"import unittest\nclass Synthetic(unittest.TestCase):\n    def test_output(self):\n        for _ in range(100000):\n            print('x'*512, flush=True)\n".into(),
                "child"=>"import subprocess\nimport sys\nimport time\nimport unittest\nclass Synthetic(unittest.TestCase):\n    def test_child(self):\n        p = subprocess.Popen([sys.executable, '-I', '-S', '-c', 'import time; time.sleep(30)'])\n        print('CHILD_PID='+str(p.pid), flush=True)\n        time.sleep(30)\n".into(),
                _=>format!("import unittest\nfrom app import add\nclass Synthetic(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(2, 3), {})\n",if case=="test_fail"{99}else{5})
            };
            f.insert("test_app.py".into(), source);
        }
        _ => return Err("Linguagem sintética desconhecida.".into()),
    }
    f.insert(
        "ordis-synthetic.json".into(),
        json!({"schema":1,"language":language,"case":case}).to_string(),
    );
    Ok(f)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureArgs {
    pub language: String,
    pub case: String,
}
pub fn fixture(root: &Path, args: Value) -> Result<Value, String> {
    let a: FixtureArgs = serde_json::from_value(args).map_err(|_| "Fixture inválida.")?;
    let expected = files(&a.language, &a.case)?;
    let name = format!("{}-{}", a.language, a.case);
    let dest = root.join(&name);
    let _guard = crate::developer::guard(root, true)?;
    fs::create_dir(&dest)
        .map_err(|_| "Fixture já existe; selecione um diretório novo. Nada será sobrescrito.")?;
    for (name, text) in expected {
        let p = dest.join(name);
        fs::create_dir_all(p.parent().unwrap()).map_err(|_| "Pasta de fixture indisponível.")?;
        fs::write(p, text).map_err(|_| "Falha ao criar fixture.")?;
    }
    Ok(json!({"project":name,"synthetic_only":true,"created":true}))
}
fn catalogue_id(id: &str) -> Result<(&str, &str), String> {
    for language in ["rust", "node", "python"] {
        if let Some(case) = id.strip_prefix(&format!("{language}-")) {
            if CASES.contains(&case) {
                return Ok((language, case));
            }
        }
    }
    Err("Projeto não pertence ao catálogo sintético autorizado.".into())
}
pub fn projects(root: &Path) -> Value {
    let mut rows = Vec::new();
    for language in ["rust", "node", "python"] {
        for case in CASES {
            let name = format!("{language}-{case}");
            if fs::symlink_metadata(root.join(&name)).is_ok_and(|m| {
                use std::os::windows::fs::MetadataExt;
                m.is_dir() && m.file_attributes() & 0x400 == 0
            }) {
                rows.push(json!({"project":name,"language":language,"case":case}));
            }
        }
    }
    json!(rows)
}
fn check_inputs(root: &Path, project: &str) -> Result<(PathBuf, BTreeMap<String, String>), String> {
    let (lang, case) = catalogue_id(project)?;
    let expected = files(lang, case)?;
    let cwd = root.join(project);
    let _guard = crate::developer::guard(&cwd, true)?;
    let mut actual = Vec::new();
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
        if dir
            .strip_prefix(root)
            .map_err(|_| "Projeto escapou do workspace.")?
            .components()
            .count()
            > 2
        {
            return Err("Diretórios não revisados no projeto.".into());
        }
        let _guard = crate::developer::guard(dir, true)?;
        for e in fs::read_dir(dir).map_err(|_| "Projeto indisponível.")? {
            let e = e.map_err(|_| "Projeto indisponível.")?;
            let p = e.path();
            if out.len() > 16 {
                return Err("Projeto excede catálogo.".into());
            }
            let meta = fs::symlink_metadata(&p).map_err(|_| "Fonte indisponível.")?;
            if meta.is_dir() {
                walk(root, &p, out)?;
            } else {
                let _g = crate::developer::guard(&p, false)?;
                out.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        Ok(())
    }
    walk(&cwd, &cwd, &mut actual)?;
    actual.sort();
    if actual != expected.keys().cloned().collect::<Vec<_>>() {
        return Err("Projeto contém arquivos não revisados; execução bloqueada.".into());
    }
    let mut hashes = BTreeMap::new();
    for (name, text) in expected {
        let p = cwd.join(&name);
        if fs::read(&p).map_err(|_| "Fonte ausente.")? != text.as_bytes() {
            return Err("Fonte mudou ou não corresponde ao catálogo confiável.".into());
        }
        hashes.insert(format!("{project}/{name}"), file_hash(&p)?);
    }
    for ancestor in cwd.ancestors() {
        for name in [".cargo/config.toml", ".cargo/config"] {
            if ancestor.join(name).exists() {
                return Err(
                    "Configuração Cargo ancestral não revisada; execução bloqueada.".into(),
                );
            }
        }
    }
    Ok((
        local_path(&cwd.canonicalize().map_err(|_| "cwd indisponível.")?)?,
        hashes,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub project: String,
    pub operation: String,
    pub reason: String,
}
pub struct Plan {
    pub proposal: Value,
    pub payload: Value,
    pub manifest: BTreeMap<String, String>,
    pub python: Tool,
    pub entry: String,
}
pub fn prepare(root: &Path, cfg: &Config, args: Value) -> Result<Plan, String> {
    validate(cfg)?;
    if !cfg.enabled {
        return Err("Jobs desativados. Habilite apenas fixtures sintéticas revisadas.".into());
    }
    let req: Request = serde_json::from_value(args).map_err(|_| {
        "Job exige projeto, operação e justificativa; argumentos livres são proibidos."
    })?;
    if !OPERATIONS.contains(&req.operation.as_str())
        || req.reason.is_empty()
        || req.reason.len() > 2000
    {
        return Err("Operação ou justificativa inválida.".into());
    }
    let (language, _) = catalogue_id(&req.project)?;
    if !req.operation.starts_with(language) {
        return Err("Operação incompatível com a linguagem do projeto.".into());
    }
    let (cwd, manifest) = check_inputs(root, &req.project)?;
    let get = |name: &str| -> Result<Tool, String> {
        let t = cfg
            .tools
            .get(name)
            .ok_or_else(|| format!("Ferramenta {name} ausente; sem instalação automática."))?
            .clone();
        if file_hash(&t.path)? != t.sha256 {
            return Err("Ferramenta mudou desde a autorização; revise a configuração.".into());
        }
        Ok(t)
    };
    let python = get("python")?;
    let binary = match req.operation.as_str() {
        "rust_fmt" => get("cargo-fmt")?,
        "rust_clippy" => {
            get("clippy-driver")?;
            get("cargo-clippy")?
        }
        s if s.starts_with("rust") => get("cargo")?,
        s if s.starts_with("node") => get("node")?,
        _ => python.clone(),
    };
    if language == "rust" {
        let rustc = get("rustc")?;
        get("rustdoc")?;
        if !rustc
            .path
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("lib/rustlib")
            .join(&cfg.target)
            .join("lib")
            .is_dir()
        {
            return Err("Biblioteca padrão do target ausente; sem downloads.".into());
        }
        if req.operation == "rust_fmt" {
            get("rustfmt")?;
        }
        if cfg.target.ends_with("gnu") {
            for n in ["gcc", "ar", "as", "dlltool", "ld"] {
                get(n)?;
            }
        }
    }
    let entry = match language {
        "rust" => "Cargo.toml",
        "node" => "app.cjs",
        _ => "app.py",
    };
    let proposal = json!({"tool":"developer_job","operation":req.operation,"project":req.project,"workspace":root,"cwd":cwd,"executable":binary.path,"reason":crate::developer::redact(&req.reason),"manifest":manifest,"runtime_sha256":binary.sha256,"limits":{"seconds":cfg.seconds,"subprocess_seconds":cfg.subprocess_seconds,"memory_mib":cfg.memory_mib,"cpu_percent":cfg.cpu_percent,"processes":cfg.processes,"storage_mib":cfg.storage_mib},"risk":"Compilação/testes executam código. Apenas catálogo sintético fixo; Job Object não é sandbox de arquivos/rede.","policy":"human approval once; installed native tools; offline; no shell"});
    let selected_tools: BTreeMap<_, _> = cfg
        .tools
        .iter()
        .filter(|(name, _)| {
            name.as_str() == "python"
                || match language {
                    "node" => name.as_str() == "node",
                    "rust" => name.as_str() != "node",
                    _ => false,
                }
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Ok(Plan {
        proposal,
        payload: json!({"operation":req.operation,"project":req.project,"cwd":cwd,"tools":selected_tools,"target":cfg.target,"limits":cfg}),
        manifest,
        python,
        entry: format!("{}/{entry}", req.project),
    })
}

static ACTIVE_PROFILES: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());
pub struct Lease(windows_sys::Win32::Foundation::HANDLE, String);
impl Lease {
    pub fn acquire(c: &Caminhos) -> Result<Self, String> {
        use windows_sys::Win32::{Foundation::*, System::Threading::*};
        let key = format!(
            "{:x}",
            Sha256::digest(
                fs::canonicalize(crate::developer::private(c)?)
                    .map_err(|_| "Perfil indisponível.")?
                    .to_string_lossy()
                    .to_lowercase()
                    .as_bytes()
            )
        );
        let mut active = ACTIVE_PROFILES
            .lock()
            .map_err(|_| "Controle de concorrência indisponível.")?;
        if active.contains(&key) {
            return Err("Outro job deste perfil está ativo.".into());
        }
        let name: Vec<u16> = format!("Local\\ORDIS.DeveloperJobs.{key}\0")
            .encode_utf16()
            .collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err("Controle de concorrência indisponível.".into());
        }
        let acquired = unsafe { WaitForSingleObject(handle, 0) };
        if acquired != WAIT_OBJECT_0 && acquired != WAIT_ABANDONED {
            unsafe {
                CloseHandle(handle);
            }
            return Err(
                "Outro job deste perfil está ativo; solicitação duplicada bloqueada.".into(),
            );
        }
        active.insert(key.clone());
        Ok(Self(handle, key))
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_PROFILES.lock() {
            active.remove(&self.1);
        }
        unsafe {
            windows_sys::Win32::System::Threading::ReleaseMutex(self.0);
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
fn root(c: &Caminhos) -> Result<PathBuf, String> {
    let p = crate::developer::private(c)?.join("jobs");
    fs::create_dir_all(&p).map_err(|_| "Diário de jobs indisponível.")?;
    let _guard = crate::developer::guard(&p, true)?;
    Ok(p)
}
fn safe_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        Err("ID de job inválido.".into())
    } else {
        Ok(())
    }
}
fn dir(c: &Caminhos, id: &str) -> Result<PathBuf, String> {
    safe_id(id)?;
    let p = root(c)?.join(id);
    if p.exists() {
        let _g = crate::developer::guard(&p, true)?;
    }
    Ok(p)
}
pub fn cancel_path(c: &Caminhos, id: &str) -> Result<PathBuf, String> {
    Ok(dir(c, id)?.join("cancel.request"))
}
fn save(c: &Caminhos, id: &str, record: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(record).map_err(|_| "Registro de job inválido.")?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Registro de job excedeu limite.".into());
    }
    crate::developer::atomic(&dir(c, id)?.join("record.json"), &bytes, true)
}
fn read_record(c: &Caminhos, id: &str) -> Result<Value, String> {
    let path = dir(c, id)?.join("record.json");
    let _g = crate::developer::guard(&path, false)?;
    if fs::metadata(&path).map_err(|_| "Job ausente.")?.len() > 2 * 1024 * 1024 {
        return Err("Registro excessivo.".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(|_| "Job ausente.")?)
        .map_err(|_| "Registro inválido.".into())
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn stage(
    c: &Caminhos,
    id: &str,
    task: &str,
    payload: &mut Value,
    proposal: &mut Value,
) -> Result<(), String> {
    let cfg: Config = serde_json::from_value(payload["limits"].clone())
        .map_err(|_| "Política do plano inválida.")?;
    validate(&cfg)?;
    let folder = dir(c, id)?;
    if fs::read_dir(root(c)?)
        .map_err(|_| "Diário indisponível.")?
        .count()
        >= 50
    {
        return Err("Limite de 50 jobs preservados atingido; nenhuma limpeza automática.".into());
    }
    let tools: BTreeMap<String, Tool> =
        serde_json::from_value(payload["tools"].clone()).map_err(|_| "Ferramentas inválidas.")?;
    let tool = |name: &str| -> Result<String, String> {
        Ok(tools
            .get(name)
            .ok_or_else(|| format!("Ferramenta {name} ausente."))?
            .path
            .to_string_lossy()
            .into_owned())
    };
    let source_cwd = PathBuf::from(payload["cwd"].as_str().ok_or("cwd ausente.")?);
    let cwd = folder.join("project");
    let op = payload["operation"]
        .as_str()
        .ok_or("Operação ausente.")?
        .to_string();
    let mut environment = BTreeMap::<String, String>::new();
    let system = std::env::var("SystemRoot").map_err(|_| "SystemRoot ausente.")?;
    for (key, value) in [
        ("SystemRoot", system.clone()),
        ("TEMP", folder.to_string_lossy().into()),
        ("TMP", folder.to_string_lossy().into()),
        (
            "CARGO_HOME",
            folder.join("cargo-home").to_string_lossy().into(),
        ),
        (
            "CARGO_TARGET_DIR",
            folder.join("target").to_string_lossy().into(),
        ),
        (
            "RUSTUP_HOME",
            folder.join("unused-rustup").to_string_lossy().into(),
        ),
        ("RUSTUP_AUTO_INSTALL", "0".into()),
        ("CARGO_NET_OFFLINE", "true".into()),
        ("CARGO_BUILD_JOBS", "1".into()),
        ("RUSTC_WRAPPER", String::new()),
        ("RUSTC_WORKSPACE_WRAPPER", String::new()),
        ("PYTHONIOENCODING", "utf-8".into()),
        ("PYTHONDONTWRITEBYTECODE", "1".into()),
    ] {
        environment.insert(key.into(), value);
    }
    let mut paths = Vec::new();
    for name in ["gcc", "link", "cargo", "rustc", "rustfmt", "python", "node"] {
        if let Some(t) = tools.get(name) {
            let p = t.path.parent().unwrap().to_string_lossy().into_owned();
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
    }
    paths.push(
        PathBuf::from(&system)
            .join("System32")
            .to_string_lossy()
            .into(),
    );
    environment.insert("PATH".into(), paths.join(";"));
    if op.starts_with("rust") {
        for (key, name) in [
            ("RUSTC", "rustc"),
            ("RUSTDOC", "rustdoc"),
            ("CARGO", "cargo"),
        ] {
            environment.insert(key.into(), tool(name)?);
        }
        environment.insert("CARGO_BUILD_TARGET".into(), cfg.target.clone());
        if let Some(t) = tools.get("rustfmt") {
            environment.insert("RUSTFMT".into(), t.path.to_string_lossy().into());
        }
        if let Some(t) = tools.get("clippy-driver") {
            environment.insert("CLIPPY_DRIVER_PATH".into(), t.path.to_string_lossy().into());
        }
        if cfg.target.ends_with("gnu") {
            environment.insert("CC".into(), tool("gcc")?);
            environment.insert("AR".into(), tool("ar")?);
            environment.insert(
                format!(
                    "CARGO_TARGET_{}_LINKER",
                    cfg.target.to_uppercase().replace('-', "_")
                ),
                tool("gcc")?,
            );
        } else {
            environment.insert(
                format!(
                    "CARGO_TARGET_{}_LINKER",
                    cfg.target.to_uppercase().replace('-', "_")
                ),
                tool("link")?,
            );
            for (k, v) in &cfg.msvc_environment {
                environment.insert(k.clone(), v.clone());
            }
        }
    }
    let path = |name: &str| cwd.join(name).to_string_lossy().into_owned();
    let command: Vec<String> = match op.as_str() {
        "rust_check" | "rust_test" | "rust_clippy" => {
            let mut v = if op == "rust_clippy" {
                vec![tool("cargo-clippy")?]
            } else {
                vec![
                    tool("cargo")?,
                    if op == "rust_test" { "test" } else { "check" }.into(),
                ]
            };
            v.extend([
                "--offline".into(),
                "--locked".into(),
                "--manifest-path".into(),
                path("Cargo.toml"),
                "--target".into(),
                cfg.target.clone(),
                "--message-format=json".into(),
            ]);
            if op == "rust_test" {
                v.extend(["--".into(), "--nocapture".into(), "--test-threads=1".into()]);
            }
            v
        }
        "rust_fmt" => vec![
            tool("cargo-fmt")?,
            "--manifest-path".into(),
            path("Cargo.toml"),
            "--check".into(),
        ],
        "node_version" => vec![tool("node")?, "--version".into()],
        "node_syntax" => vec![tool("node")?, "--check".into(), path("app.cjs")],
        "node_test" => vec![tool("node")?, "--test".into(), path("test_app.cjs")],
        "python_syntax" => vec![
            tool("python")?,
            "-I".into(),
            "-S".into(),
            "-B".into(),
            "-X".into(),
            "utf8".into(),
            "-X".into(),
            format!("pycache_prefix={}", folder.join("pycache").display()),
            "-m".into(),
            "py_compile".into(),
            path("app.py"),
            path("test_app.py"),
        ],
        "python_test" => vec![
            tool("python")?,
            "-I".into(),
            "-S".into(),
            "-B".into(),
            "-X".into(),
            "utf8".into(),
            "-m".into(),
            "unittest".into(),
            "discover".into(),
            "-s".into(),
            cwd.to_string_lossy().into(),
            "-p".into(),
            "test_app.py".into(),
            "-v".into(),
        ],
        _ => return Err("Operação recusada.".into()),
    };
    fs::create_dir(&folder).map_err(|_| "ID já existe; job duplicado bloqueado.".to_string())?;
    let project = payload["project"].as_str().ok_or("Projeto ausente.")?;
    let (language, case) = catalogue_id(project)?;
    fs::create_dir(&cwd).map_err(|_| "Snapshot sintético já existe.")?;
    for (name, text) in files(language, case)? {
        let file = cwd.join(name);
        fs::create_dir_all(file.parent().unwrap()).map_err(|_| "Snapshot indisponível.")?;
        fs::write(file, text.as_bytes()).map_err(|_| "Snapshot indisponível.")?;
    }
    for ancestor in cwd.ancestors() {
        if ancestor.join(".cargo/config.toml").exists() || ancestor.join(".cargo/config").exists() {
            return Err("Configuração Cargo ancestral não revisada.".into());
        }
    }
    payload["source_cwd"] = json!(source_cwd);
    payload["cwd"] = json!(cwd);
    proposal["source_cwd"] = json!(source_cwd);
    proposal["cwd"] = json!(cwd);
    proposal["input_policy"] = json!(
        "Executa uma cópia privada da fixture sintética revisada. O workspace de origem não é executado diretamente."
    );
    payload["command"] = json!(command);
    payload["environment"] = json!(environment);
    payload["run_limits"] = json!({"seconds":cfg.seconds,"subprocess_seconds":cfg.subprocess_seconds,"memory_mib":cfg.memory_mib,"cpu_percent":cfg.cpu_percent,"processes":cfg.processes,"storage_mib":cfg.storage_mib,"storage_dir":folder,"cancel_file":folder.join("cancel.request")});
    proposal["arguments"] = json!(command.iter().skip(1).collect::<Vec<_>>());
    proposal["job_id"] = json!(id);
    let owner_created = crate::win::creation_time(std::process::id())
        .ok_or("Identidade do processo indisponível.")?;
    let mut record = json!({"schema":1,"id":id,"task":task,"operation":op,"project":payload["project"],"workspace":proposal["workspace"],"cwd":cwd,"executable":command[0],"arguments":command.iter().skip(1).collect::<Vec<_>>(),"limits":payload["run_limits"],"tool_hashes":tools,"approval_state":"pending","state":"pending","owner_pid":std::process::id(),"owner_created":owner_created,"created_ms":now_ms(),"exit_code":null,"summary":null,"audit_ref":format!("operations/{id}"),"result":null});
    save(c, id, &record)?;
    record["state"] = json!("awaiting_approval");
    save(c, id, &record)
}
pub fn approval_end(c: &Caminhos, id: &str, state: &str) -> Result<(), String> {
    let mut r = read_record(c, id)?;
    r["approval_state"] = json!(if state == "timeout" {
        "expired"
    } else if state == "cancelled" {
        "cancelled"
    } else {
        "denied"
    });
    r["state"] = json!(if state == "timeout" {
        "timed_out"
    } else if state == "cancelled" {
        "cancelled"
    } else {
        "failed"
    });
    r["summary"] = json!({"reason":format!("approval_{state}"),"executed":false});
    save(c, id, &r)
}
pub fn failed(c: &Caminhos, id: &str, reason: &str) -> Result<(), String> {
    let mut r = read_record(c, id)?;
    let executed = if matches!(r["state"].as_str(), Some("pending" | "awaiting_approval")) {
        json!(false)
    } else {
        Value::Null
    };
    r["state"] = json!("failed");
    r["summary"] = json!({"executed":executed,"reason":crate::developer::redact(reason)});
    save(c, id, &r)
}
/// Finalize early-return failures; a live GUI must never leave a phantom running job.
pub struct RecordGuard<'a> {
    c: &'a Caminhos,
    id: String,
}
impl<'a> RecordGuard<'a> {
    pub fn new(c: &'a Caminhos, id: &str) -> Self {
        Self { c, id: id.into() }
    }
}
impl Drop for RecordGuard<'_> {
    fn drop(&mut self) {
        if let Ok(r) = read_record(self.c, &self.id) {
            if matches!(
                r["state"].as_str(),
                Some("pending" | "awaiting_approval" | "running")
            ) {
                let _ = failed(
                    self.c,
                    &self.id,
                    "Executor retornou antes de concluir o job; nenhuma repetição automática.",
                );
            }
        }
    }
}
pub fn running(c: &Caminhos, id: &str) -> Result<(), String> {
    let mut r = read_record(c, id)?;
    if r["state"] != "awaiting_approval" {
        return Err("Job duplicado, expirado ou já encerrado.".into());
    }
    r["approval_state"] = json!("allowed");
    r["approved_ms"] = json!(now_ms());
    r["state"] = json!("running");
    save(c, id, &r)
}
pub fn finish(c: &Caminhos, id: &str, raw: Value) -> Result<Value, String> {
    let raw_state = raw["state"].as_str().unwrap_or("failed");
    let state = match raw_state {
        "completed" => "completed",
        "timeout" => "timed_out",
        "cancelled" => "cancelled",
        _ => "failed",
    };
    let mut record = read_record(c, id)?;
    let stdout = raw["stdout"].as_str().unwrap_or("");
    let stderr = raw["stderr"].as_str().unwrap_or("");
    let diagnostics = diagnostics(stdout, stderr);
    let result = json!({"state":state,"job_id":id,"exit_code":raw["exit_code"],"duration_ms":raw["duration_ms"],"trusted_executor":{"state":state,"exit_code":raw["exit_code"],"reason":raw_state,"owned_pids":raw["owned_pids"],"peak_job_memory_bytes":raw["peak_job_memory_bytes"],"audit_ref":record["audit_ref"]},"untrusted_output":{"stdout":stdout,"stderr":stderr,"truncated":raw["truncated"]},"untrusted_diagnostics":diagnostics,"summary":{"success":state=="completed","operation":record["operation"],"reason":raw_state}});
    record["state"] = json!(state);
    record["exit_code"] = raw["exit_code"].clone();
    record["summary"] = result["summary"].clone();
    record["result"] = result.clone();
    record["finished_ms"] = json!(now_ms());
    save(c, id, &record)?;
    Ok(result)
}
fn diagnostics(stdout: &str, stderr: &str) -> Value {
    let mut entries = Vec::new();
    let mut counts = BTreeMap::<String, u64>::new();
    for line in stdout.lines().chain(stderr.lines()).take(4096) {
        if entries.len() < 64 {
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                if v["reason"] == "compiler-message" {
                    let m = &v["message"];
                    let span = m["spans"]
                        .as_array()
                        .and_then(|a| a.iter().find(|s| s["is_primary"] == true));
                    entries.push(json!({"untrusted":true,"level":m["level"],"code":m["code"]["code"],"message":m["message"].as_str().unwrap_or("").chars().take(2048).collect::<String>(),"file":span.map(|s|s["file_name"].clone()),"line":span.map(|s|s["line_start"].clone()),"column":span.map(|s|s["column_start"].clone())}));
                }
            }
        }
        for (prefix, key) in [
            ("# pass ", "passed"),
            ("# fail ", "failed"),
            ("# tests ", "tests"),
        ] {
            if let Some(n) = line
                .trim()
                .strip_prefix(prefix)
                .and_then(|s| s.parse::<u64>().ok())
            {
                counts.insert(key.into(), n);
            }
        }
        if line.contains("test result:") {
            for part in line.split(';') {
                for (key, word) in [("passed", "passed"), ("failed", "failed")] {
                    if let Some(index) = part.find(word) {
                        if let Some(n) = part[..index]
                            .split_whitespace()
                            .last()
                            .and_then(|s| s.parse().ok())
                        {
                            counts.insert(key.into(), n);
                        }
                    }
                }
            }
        }
        if let Some(n) = line
            .trim()
            .strip_prefix("Ran ")
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
        {
            counts.insert("tests".into(), n);
        }
        if entries.len() < 64 && (line.contains("SyntaxError") || line.contains("AssertionError")) {
            entries.push(json!({"untrusted":true,"level":"error","message":line.chars().take(2048).collect::<String>()}));
        }
    }
    json!({"items":entries,"test_counts":counts,"trusted":false,"instruction_authority":false})
}
pub fn history(c: &Caminhos) -> Result<Value, String> {
    let mut rows = Vec::new();
    for e in fs::read_dir(root(c)?)
        .map_err(|_| "Diário indisponível.")?
        .take(50)
    {
        let e = e.map_err(|_| "Diário indisponível.")?;
        let id = e.file_name().to_string_lossy().into_owned();
        if safe_id(&id).is_err() {
            continue;
        }
        if let Ok(mut r) = read_record(c, &id) {
            let active = matches!(
                r["state"].as_str(),
                Some("pending" | "awaiting_approval" | "running")
            );
            let pid = r["owner_pid"].as_u64().unwrap_or(0) as u32;
            let start = r["owner_created"].as_f64().unwrap_or(0.0);
            if active
                && (!crate::win::is_running(pid)
                    || crate::win::creation_time(pid).is_none_or(|t| (t - start).abs() > 0.01))
            {
                r["state"] = json!("interrupted");
                r["approval_state"] = json!("expired");
                r["summary"] = json!({"reason":"owner_interrupted","automatic_retry":false});
                save(c, &id, &r)?;
            }
            r["result"] = Value::Null;
            rows.push(r);
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r["created_ms"].as_u64().unwrap_or(0)));
    Ok(json!({"jobs":rows,"automatic_execution":false,"limit":50}))
}
pub fn action(c: &Caminhos, name: &str, args: Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Id {
        id: String,
    }
    match name {
        "job_history" => {
            if args.as_object().is_none_or(|a| !a.is_empty()) {
                return Err("Histórico não aceita argumentos.".into());
            }
            history(c)
        }
        "job_get" | "job_logs" | "job_cancel" => {
            let a: Id = serde_json::from_value(args).map_err(|_| "ID inválido.")?;
            let _ = history(c)?;
            let r = read_record(c, &a.id)?;
            if name == "job_cancel" {
                if !matches!(
                    r["state"].as_str(),
                    Some("pending" | "awaiting_approval" | "running")
                ) {
                    return Err("Job já encerrado.".into());
                }
                let path = cancel_path(c, &a.id)?;
                if !path.exists() {
                    crate::developer::atomic(&path, b"human cancellation\n", false)?;
                }
                Ok(json!({"cancel_requested":true,"id":a.id}))
            } else if name == "job_logs" {
                Ok(json!({"id":a.id,"untrusted":true,"output":r["result"]["untrusted_output"]}))
            } else {
                Ok(r)
            }
        }
        _ => Err("Ação de job desconhecida.".into()),
    }
}
pub fn seal_snapshot(payload: &Value) -> Result<Vec<File>, String> {
    let path = PathBuf::from(payload["cwd"].as_str().ok_or("Snapshot ausente.")?);
    let (lang, case) = catalogue_id(payload["project"].as_str().ok_or("Projeto ausente.")?)?;
    let mut manifest = BTreeMap::new();
    for (name, text) in files(lang, case)? {
        let file = path.join(&name);
        if fs::read(&file).map_err(|_| "Snapshot ausente.")? != text.as_bytes() {
            return Err("Snapshot sintético foi alterado.".into());
        }
        manifest.insert(name, file_hash(&file)?);
    }
    seal(&path, &manifest, &json!({}))
}

pub fn seal(
    root: &Path,
    manifest: &BTreeMap<String, String>,
    tools: &Value,
) -> Result<Vec<File>, String> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut locks = vec![crate::developer::guard(root, true)?];
    for (name, digest) in manifest {
        let p = root.join(name);
        let mut parent = p.parent();
        while let Some(dir) = parent {
            if !dir.starts_with(root) {
                return Err("Fonte fora do workspace.".into());
            }
            locks.push(crate::developer::guard(dir, true)?);
            if dir == root {
                break;
            }
            parent = dir.parent();
        }
        let _guard = crate::developer::guard(&p, false)?;
        let f = fs::OpenOptions::new()
            .read(true)
            .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
            .open(&p)
            .map_err(|_| "Fonte ocupada para escrita; execução bloqueada.")?;
        if file_hash(&p)? != *digest {
            return Err("Fonte mudou depois da aprovação.".into());
        }
        locks.push(f);
    }
    let tools: BTreeMap<String, Tool> =
        serde_json::from_value(tools.clone()).map_err(|_| "Registro de ferramentas inválido.")?;
    for t in tools.values() {
        let _g = crate::developer::guard(&t.path, false)?;
        let f = fs::OpenOptions::new()
            .read(true)
            .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
            .open(&t.path)
            .map_err(|_| "Ferramenta ocupada para escrita.".to_string())?;
        if file_hash(&t.path)? != t.sha256 {
            return Err("Ferramenta mudou depois da aprovação.".into());
        }
        locks.push(f);
    }
    Ok(locks)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Caminhos {
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("Lucille/ORDIS-Teste")
            .join(format!(
                "jobs-synthetic-{}",
                crate::controle::sufixo_unico()
            ));
        fs::create_dir_all(&root).unwrap();
        let c = Caminhos {
            projeto: root.clone(),
            runtime: root.join("runtime"),
        };
        assert!(crate::memory::database_path(&c).unwrap().starts_with(&root));
        c
    }
    #[test]
    fn profile_lease_rejects_recursive_and_concurrent_owners() {
        let c = profile();
        let lease = Lease::acquire(&c).unwrap();
        assert!(Lease::acquire(&c).is_err());
        std::thread::scope(|s| {
            s.spawn(|| assert!(Lease::acquire(&c).is_err()))
                .join()
                .unwrap();
        });
        drop(lease);
        assert!(Lease::acquire(&c).is_ok());
        fs::remove_dir_all(c.projeto).unwrap();
    }
    #[test]
    fn dead_owner_is_interrupted_without_retry_or_cancellation() {
        let c = profile();
        let id = "abcdef012345";
        fs::create_dir(dir(&c, id).unwrap()).unwrap();
        save(&c,id,&json!({"id":id,"state":"running","owner_pid":std::process::id(),"owner_created":0.0,"created_ms":1})).unwrap();
        let result = history(&c).unwrap();
        assert_eq!(result["jobs"][0]["state"], "interrupted");
        assert_eq!(result["automatic_execution"], false);
        assert!(action(&c, "job_cancel", json!({"id":id})).is_err());
        assert!(!cancel_path(&c, id).unwrap().exists());
        assert!(action(&c, "job_get", json!({"id":"../escape"})).is_err());
        fs::remove_dir_all(c.projeto).unwrap();
    }
    #[test]
    fn extra_files_and_hard_link_sources_are_rejected() {
        let c = profile();
        fixture(&c.projeto, json!({"language":"node","case":"pass"})).unwrap();
        let file = c.projeto.join("node-pass/app.cjs");
        let alias = c.projeto.join("alias.cjs");
        fs::hard_link(&file, &alias).unwrap();
        assert!(check_inputs(&c.projeto, "node-pass").is_err());
        fs::remove_file(alias).unwrap();
        fs::write(c.projeto.join("node-pass/extra.cjs"), "unreviewed").unwrap();
        assert!(check_inputs(&c.projeto, "node-pass").is_err());
        fs::remove_dir_all(c.projeto).unwrap();
    }
    #[test]
    fn structured_diagnostics_have_no_instruction_authority() {
        let cargo = json!({"reason":"compiler-message","message":{"level":"error","code":{"code":"E0425"},"message":"Synthetic error","spans":[{"is_primary":true,"file_name":"src/lib.rs","line_start":2,"column_start":3}]}});
        let d = diagnostics(&cargo.to_string(), "Ran 1 test\nFAILED (failures=1)");
        assert_eq!(d["trusted"], false);
        assert_eq!(d["instruction_authority"], false);
        assert_eq!(d["items"][0]["code"], "E0425");
        assert_eq!(d["items"][0]["line"], 2);
    }
    #[test]
    fn missing_tools_disabled_jobs_and_old_approvals_fail_closed() {
        let c = profile();
        fixture(&c.projeto, json!({"language":"python","case":"pass"})).unwrap();
        let mut cfg = Config::default();
        let args = json!({"project":"python-pass","operation":"python_test","reason":"Synthetic"});
        assert!(prepare(&c.projeto, &cfg, args.clone()).is_err());
        cfg.enabled = true;
        assert!(prepare(&c.projeto, &cfg, args).is_err());
        let id = "012345abcdef";
        fs::create_dir(dir(&c, id).unwrap()).unwrap();
        save(
            &c,
            id,
            &json!({"id":id,"state":"awaiting_approval","approval_state":"pending"}),
        )
        .unwrap();
        running(&c, id).unwrap();
        assert!(running(&c, id).is_err());
        approval_end(&c, id, "timeout").unwrap();
        assert!(running(&c, id).is_err());
        fs::remove_dir_all(c.projeto).unwrap();
    }
    #[test]
    fn catalogue_is_closed_and_source_changes_are_rejected() {
        let root = std::env::temp_dir().join(format!(
            "ordis-job-catalogue-{}",
            crate::controle::sufixo_unico()
        ));
        fs::create_dir(&root).unwrap();
        fixture(&root, json!({"language":"rust","case":"pass"})).unwrap();
        assert!(check_inputs(&root, "rust-pass").is_ok());
        fs::write(root.join("rust-pass/src/lib.rs"), "unreviewed source").unwrap();
        assert!(check_inputs(&root, "rust-pass").is_err());
        assert!(catalogue_id("../escape").is_err());
        assert!(fixture(&root, json!({"language":"rust","case":"pass"})).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn policy_limits_and_unstructured_commands_fail_closed() {
        let mut c = Config::default();
        assert!(validate(&c).is_ok());
        c.processes = 100;
        assert!(validate(&c).is_err());
        assert!(serde_json::from_value::<Request>(json!({"project":"node-pass","operation":"node_test","reason":"test","arguments":["install"]})).is_err());
    }
}
