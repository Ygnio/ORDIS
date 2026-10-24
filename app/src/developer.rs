//! Workspace capabilities, user-only approvals and local task journal.
use crate::controle::Caminhos;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, MoveFileExW, BY_HANDLE_FILE_INFORMATION,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_READ, FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub permissions: crate::settings::Developer,
    pub max_file_bytes: usize,
    pub max_read_bytes: usize,
    pub max_output_bytes: usize,
    pub max_calls: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            permissions: Default::default(),
            max_file_bytes: 512 * 1024,
            max_read_bytes: 24000,
            max_output_bytes: 64000,
            max_calls: 16,
        }
    }
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn private(c: &Caminhos) -> Result<PathBuf, String> {
    let dir = crate::memory::database_path(c)?
        .parent()
        .ok_or("Pasta privada inválida.")?
        .join("developer-data");
    fs::create_dir_all(&dir).map_err(|_| "Pasta privada do desenvolvedor indisponível.")?;
    let parent = dir
        .parent()
        .unwrap()
        .canonicalize()
        .map_err(|_| "Raiz privada indisponível.")?;
    if !dir
        .canonicalize()
        .map_err(|_| "Pasta privada indisponível.")?
        .starts_with(parent)
    {
        return Err("Pasta privada redirecionada para fora da instalação.".into());
    }
    Ok(dir)
}
pub fn load(c: &Caminhos) -> Config {
    private(c)
        .ok()
        .and_then(|p| fs::read(p.join("developer-config.json")).ok())
        .and_then(|b| serde_json::from_slice::<Config>(&b).ok())
        .and_then(|v| validate(c, v).ok())
        .unwrap_or_default()
}
fn validate(c: &Caminhos, v: Config) -> Result<Config, String> {
    let p = &v.permissions;
    if p.allow_commit
        || p.allow_push
        || p.confirmation != "always"
        || !p.isolated_workspace
        || !p.backups
        || !p.local_logs
    {
        return Err("Esta fase exige aprovação por operação, isolamento, histórico e backups; Git permanece bloqueado.".into());
    }
    if !(1..=600).contains(&p.timeout_seconds)
        || !(1..=64).contains(&v.max_calls)
        || !(1024..=8 * 1024 * 1024).contains(&v.max_file_bytes)
        || !(512..=128000).contains(&v.max_read_bytes)
        || !(512..=256000).contains(&v.max_output_bytes)
    {
        return Err("Limites fora do intervalo permitido.".into());
    }
    if p.enabled {
        let root = Path::new(&p.workspace)
            .canonicalize()
            .map_err(|_| "Selecione um workspace existente.")?;
        let home = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .and_then(|p| p.canonicalize().ok());
        let system = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .and_then(|p| p.canonicalize().ok());
        if !root.is_dir()
            || root.parent().is_none()
            || home.as_ref() == Some(&root)
            || system.as_ref().is_some_and(|s| root.starts_with(s))
            || private(c)?.starts_with(&root)
            || root.starts_with(private(c)?.parent().unwrap())
            || c.projeto.canonicalize().ok().as_ref() == Some(&root)
            || std::env::current_exe()
                .ok()
                .and_then(|p| p.canonicalize().ok())
                .is_some_and(|exe| exe.starts_with(&root))
        {
            return Err("Use um projeto separado; raiz do disco, perfil, Windows e dados privados não são workspaces.".into());
        }
        if fs::symlink_metadata(&p.workspace)
            .map_err(|_| "Workspace indisponível.")?
            .file_attributes()
            & FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err("Workspace não pode ser um link/junction.".into());
        }
    }
    Ok(v)
}
fn db(c: &Caminhos) -> Result<Connection, String> {
    let db = Connection::open(private(c)?.join("developer-state.sqlite3"))
        .map_err(|_| "Histórico privado indisponível.")?;
    db.busy_timeout(Duration::from_millis(250))
        .map_err(|_| "Histórico ocupado.")?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY,title TEXT NOT NULL,state TEXT NOT NULL,owner INTEGER,started REAL,updated INTEGER); CREATE TABLE IF NOT EXISTS operations(id TEXT PRIMARY KEY,task TEXT NOT NULL,tool TEXT NOT NULL,state TEXT NOT NULL,proposal TEXT NOT NULL,result TEXT NOT NULL,updated INTEGER NOT NULL);").map_err(|_|"Falha ao preparar histórico privado.")?;
    Ok(db)
}
pub fn begin(c: &Caminhos, id: &str, title: &str) {
    if let Ok(mut refused) = REFUSED.lock() {
        refused.retain(|v| v != id);
    }
    if !load(c).permissions.enabled {
        return;
    }
    if let Ok(db) = db(c) {
        let title = if crate::memory::has_secret(title) {
            "Tarefa local"
        } else {
            title
        };
        let _=db.execute("INSERT INTO tasks VALUES(?1,?2,'running',?3,?4,?5) ON CONFLICT(id) DO UPDATE SET state='running',owner=excluded.owner,started=excluded.started,updated=excluded.updated",params![id,title.chars().take(256).collect::<String>(),std::process::id(),crate::win::creation_time(std::process::id()).unwrap_or(0.0),chrono::Utc::now().timestamp()]);
    }
}
pub fn finish(c: &Caminhos, id: &str, state: &str) {
    if !load(c).permissions.enabled {
        return;
    }
    if let Ok(db) = db(c) {
        let _ = db.execute(
            "UPDATE tasks SET state=?2,updated=?3 WHERE id=?1",
            params![id, state, chrono::Utc::now().timestamp()],
        );
    }
}
fn journal(
    c: &Caminhos,
    id: &str,
    task: &str,
    tool: &str,
    state: &str,
    proposal: &Value,
    result: &Value,
) -> Result<(), String> {
    db(c)?.execute("INSERT INTO operations VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET state=excluded.state,result=excluded.result,updated=excluded.updated",params![id,task,tool,state,proposal.to_string(),result.to_string(),chrono::Utc::now().timestamp()]).map_err(|_|"Falha ao registrar operação; não será executada sem auditoria.")?;
    Ok(())
}
pub fn redact(text: &str) -> String {
    text.lines()
        .map(|l| {
            if crate::memory::has_secret(l) {
                "[linha com possível segredo ocultada]"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn sensitive(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        ".git",
        ".ssh",
        ".aws",
        ".azure",
        ".codex",
        "runtime",
        "developer-data",
        "private",
        "backups",
        "profiles",
        "pending-records",
    ]
    .contains(&n.as_str())
        || n.starts_with(".env")
        || n == "agents.local.md"
        || n == "api_tokens.yml"
        || [
            ".sqlite", ".sqlite3", ".db", ".pem", ".pfx", ".key", ".dpapi",
        ]
        .iter()
        .any(|e| n.ends_with(e))
        || n.ends_with("-wal")
        || n.ends_with("-shm")
}
struct Scope {
    root: PathBuf,
    max_file: usize,
    max_read: usize,
}
struct Resolved {
    path: PathBuf,
    _locks: Vec<File>,
}
fn guard(path: &Path, directory: bool) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(
            FILE_FLAG_OPEN_REPARSE_POINT
                | if directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
        );
    let f = options
        .open(path)
        .map_err(|_| "Não foi possível bloquear/abrir o caminho.")?;
    let m = f.metadata().map_err(|_| "Metadados indisponíveis.")?;
    if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err("Links e junctions não são permitidos nesta fase.".into());
    }
    if !directory {
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(f.as_raw_handle() as _, &mut info) } == 0
            || info.nNumberOfLinks > 1
        {
            return Err("Hard links ou metadados não verificáveis são recusados.".into());
        }
    }
    Ok(f)
}
impl Scope {
    fn new(c: &Caminhos, cfg: &Config) -> Result<Self, String> {
        if !cfg.permissions.enabled {
            return Err("Modo Desenvolvedor desativado.".into());
        }
        validate(c, cfg.clone())?;
        Ok(Self {
            root: Path::new(&cfg.permissions.workspace)
                .canonicalize()
                .map_err(|_| "Workspace indisponível.")?,
            max_file: cfg.max_file_bytes,
            max_read: cfg.max_read_bytes,
        })
    }
    fn path(&self, input: &str, allow_missing: bool) -> Result<Resolved, String> {
        if input.len() > 4096 || input.contains(['\0', '\n', '\r']) {
            return Err("Caminho inválido.".into());
        }
        let raw = Path::new(input);
        if raw.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err("Traversal recusado.".into());
        }
        let relative = if raw.is_absolute() {
            let canonical = if raw.exists() {
                raw.canonicalize()
            } else {
                raw.parent()
                    .ok_or("Pai inexistente.")?
                    .canonicalize()
                    .map(|p| p.join(raw.file_name().unwrap_or_default()))
            }
            .map_err(|_| "Caminho indisponível.")?;
            canonical
                .strip_prefix(&self.root)
                .map_err(|_| "Caminho fora do workspace.")?
                .to_path_buf()
        } else {
            raw.to_path_buf()
        };
        let mut path = self.root.clone();
        let mut locks = vec![guard(&path, true)?];
        let mut components = relative.components().peekable();
        while let Some(component) = components.next() {
            let part = match component {
                Component::CurDir => continue,
                Component::Normal(p) => p.to_string_lossy().to_string(),
                _ => return Err("Caminho deve estar no workspace.".into()),
            };
            let stem = part.split('.').next().unwrap_or("").to_uppercase();
            if part.contains(':')
                || part.ends_with(['.', ' '])
                || sensitive(&part)
                || [
                    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                    "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
                    "LPT8", "LPT9",
                ]
                .contains(&stem.as_str())
            {
                return Err("Caminho privado, reservado ou protegido.".into());
            }
            path.push(part);
            if path.exists() || fs::symlink_metadata(&path).is_ok() {
                let directory = fs::symlink_metadata(&path)
                    .map_err(|_| "Caminho indisponível.")?
                    .is_dir();
                locks.push(guard(&path, directory)?);
                if !directory && components.peek().is_some() {
                    return Err("Pai não é diretório.".into());
                }
            } else if !allow_missing || components.peek().is_some() {
                return Err("Arquivo ou diretório inexistente.".into());
            }
        }
        Ok(Resolved {
            path,
            _locks: locks,
        })
    }
    fn bytes(&self, input: &str) -> Result<Vec<u8>, String> {
        let p = self.path(input, false)?;
        if fs::metadata(&p.path)
            .map_err(|_| "Arquivo inexistente.")?
            .len()
            > self.max_file as u64
        {
            return Err("Arquivo excede o limite configurado para edição/hash.".into());
        }
        let b = fs::read(&p.path).map_err(|_| "Falha ao ler arquivo.")?;
        if b.contains(&0) || std::str::from_utf8(&b).is_err() {
            return Err("Arquivo binário ou não UTF-8.".into());
        }
        if crate::memory::has_secret(std::str::from_utf8(&b).unwrap()) {
            return Err("Arquivo contém possível segredo; acesso automático recusado.".into());
        }
        Ok(b)
    }
    fn files(&self, dir: &str) -> Result<Vec<String>, String> {
        let p = self.path(dir, false)?;
        let mut result = Vec::new();
        self.walk(&p.path, 0, &mut result)?;
        Ok(result)
    }
    fn walk(&self, path: &Path, depth: usize, out: &mut Vec<String>) -> Result<(), String> {
        let _guard = guard(path, true)?;
        if depth > 8 || out.len() >= 2000 {
            return Ok(());
        }
        for e in fs::read_dir(path)
            .map_err(|_| "Não foi possível listar diretório.")?
            .flatten()
        {
            if out.len() >= 2000 {
                break;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if sensitive(&name)
                || ["target", "node_modules", ".venv", "__pycache__"].contains(&name.as_str())
            {
                continue;
            }
            let m = fs::symlink_metadata(e.path()).map_err(|_| "Metadados indisponíveis.")?;
            if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                continue;
            }
            let relative = e
                .path()
                .strip_prefix(&self.root)
                .map_err(|_| "Caminho fora do workspace.")?
                .to_string_lossy()
                .to_string();
            if m.is_dir() {
                self.walk(&e.path(), depth + 1, out)?;
            } else {
                out.push(relative);
            }
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    path: String,
    #[serde(default = "one")]
    offset: usize,
    #[serde(default = "lines_default")]
    limit: usize,
}
fn one() -> usize {
    1
}
fn lines_default() -> usize {
    120
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArgs {
    #[serde(default = "dot")]
    path: String,
}
fn dot() -> String {
    ".".into()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    #[serde(default = "dot")]
    path: String,
    #[serde(default)]
    mode: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArgs {
    path: String,
    content: String,
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchArgs {
    path: String,
    expected_sha256: String,
    old: String,
    new: String,
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MutateArgs {
    path: String,
    #[serde(default)]
    destination: Option<String>,
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecuteArgs {
    executable: String,
    arguments: Vec<String>,
    #[serde(default = "dot")]
    cwd: String,
    reason: String,
}
fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|_| "Argumentos inválidos ou campos não autorizados.".into())
}
fn read(scope: &Scope, a: ReadArgs) -> Result<Value, String> {
    if a.offset == 0 || a.offset > 1_000_000 || a.limit == 0 || a.limit > 2000 {
        return Err("Intervalo de linhas inválido.".into());
    }
    let p = scope.path(&a.path, false)?;
    let mut reader = BufReader::new(
        File::open(&p.path)
            .map_err(|_| "Falha ao ler arquivo.")?
            .take(32 * 1024 * 1024),
    );
    let mut line = Vec::new();
    let (mut lines, mut used, mut n) = (Vec::new(), 0, 0);
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|_| "Leitura parcial falhou.")?
            == 0
        {
            break;
        }
        n += 1;
        if line.len() > 128000 || line.contains(&0) {
            return Err("Linha muito longa ou arquivo binário.".into());
        }
        if n < a.offset {
            continue;
        }
        let text = std::str::from_utf8(&line).map_err(|_| "Arquivo não UTF-8.")?;
        if used + text.len() > scope.max_read {
            break;
        }
        used += text.len();
        lines.push(json!({"line":n,"text":redact(text)}));
        if lines.len() >= a.limit {
            break;
        }
    }
    let digest = scope.bytes(&a.path).ok().map(|b| hash(&b));
    Ok(
        json!({"path":a.path,"lines":lines,"sha256":digest,"limited":used>=scope.max_read||n>=a.offset+a.limit-1,"untrusted":true}),
    )
}
struct Prepared {
    proposal: Value,
    kind: String,
    path: String,
    destination: Option<String>,
    before: Option<String>,
    after: Option<Vec<u8>>,
    payload: Value,
    exe: Option<PathBuf>,
    exe_hash: Option<String>,
    manifest: BTreeMap<String, String>,
    cwd: String,
}
fn prepare(scope: &Scope, cfg: &Config, name: &str, args: Value) -> Result<Prepared, String> {
    let mut p = Prepared {
        proposal: json!({}),
        kind: name.into(),
        path: String::new(),
        destination: None,
        before: None,
        after: None,
        payload: json!({}),
        exe: None,
        exe_hash: None,
        manifest: BTreeMap::new(),
        cwd: ".".into(),
    };
    let reason;
    match name {
        "workspace_write" => {
            let a: WriteArgs = parse(args)?;
            reason = a.reason;
            p.path = a.path;
            if scope.path(&p.path, true)?.path.exists() {
                return Err("Arquivo já existe; use workspace_patch com hash.".into());
            }
            p.after = Some(a.content.into_bytes());
        }
        "workspace_patch" => {
            let a: PatchArgs = parse(args)?;
            reason = a.reason;
            p.path = a.path;
            let bytes = scope.bytes(&p.path)?;
            if hash(&bytes) != a.expected_sha256 {
                return Err("Conflito: hash não corresponde ao arquivo atual.".into());
            }
            let text = String::from_utf8(bytes).unwrap();
            if a.old.is_empty() || text.matches(&a.old).count() != 1 {
                return Err("Patch exige exatamente uma ocorrência do trecho antigo.".into());
            }
            p.before = Some(a.expected_sha256);
            p.after = Some(text.replacen(&a.old, &a.new, 1).into_bytes());
        }
        "workspace_mkdir" | "workspace_move" | "workspace_delete" => {
            let a: MutateArgs = parse(args)?;
            reason = a.reason;
            p.path = a.path;
            p.destination = a.destination;
            let target = scope.path(&p.path, name == "workspace_mkdir")?;
            if target.path == scope.root {
                return Err("A raiz do workspace é protegida.".into());
            }
            if name != "workspace_mkdir" {
                if target.path.is_dir() {
                    if fs::read_dir(&target.path)
                        .map_err(|_| "Diretório indisponível.")?
                        .next()
                        .is_some()
                    {
                        return Err(
                            "Somente arquivos ou diretórios vazios podem ser movidos/excluídos."
                                .into(),
                        );
                    }
                } else {
                    p.before = Some(hash(&scope.bytes(&p.path)?));
                }
            }
            if name == "workspace_move" {
                let dest = p.destination.as_ref().ok_or("Informe destination.")?;
                if scope.path(dest, true)?.path.exists() {
                    return Err("Destino já existe.".into());
                }
            }
        }
        "developer_execute" => {
            let a: ExecuteArgs = parse(args)?;
            reason = a.reason;
            p.cwd = a.cwd;
            scope.path(&p.cwd, false)?;
            if a.arguments.len() != 1 {
                return Err("Nesta fase use um único script relativo (.py/.ps1), sem flags ou código de shell.".into());
            }
            let configured=match a.executable.as_str(){"python"=>&cfg.permissions.python,"powershell"=>&cfg.permissions.shell,_=>return Err("Execução disponível somente para Python/PowerShell restritos; outras ferramentas são detectadas, não executadas.".into())};
            let exe = Path::new(configured)
                .canonicalize()
                .map_err(|_| "Configure o caminho absoluto do runtime instalado.")?;
            if !Path::new(configured).is_absolute() || !exe.is_file() {
                return Err("Runtime deve ser um executável absoluto instalado.".into());
            }
            let _guard = guard(&exe, false)?;
            p.exe_hash = Some(hash(
                &fs::read(&exe).map_err(|_| "Não foi possível verificar runtime.")?,
            ));
            p.exe = Some(exe);
            p.path = a.arguments[0].clone();
            let entry = Path::new(&p.path);
            if entry.components().count() != 1 {
                return Err(
                    "Scripts de entrada devem estar na raiz do workspace nesta fase.".into(),
                );
            }
            if a.executable == "python" {
                if entry.extension().is_none_or(|e| e != "py") {
                    return Err("Selecione um script Python .py.".into());
                }
                let mut sources = BTreeMap::new();
                let mut bytes_total = 0;
                for file in scope.files(".")? {
                    if !file.ends_with(".py") || Path::new(&file).components().count() != 1 {
                        continue;
                    }
                    let bytes = scope.bytes(&file)?;
                    bytes_total += bytes.len();
                    if sources.len() >= 128 || bytes_total > 2 * 1024 * 1024 {
                        return Err("Manifesto Python excede 128 arquivos/2 MiB.".into());
                    }
                    p.manifest.insert(file.clone(), hash(&bytes));
                    sources.insert(file, String::from_utf8(bytes).unwrap());
                }
                if !sources.contains_key(&p.path) {
                    return Err("Script de entrada inexistente.".into());
                }
                p.payload = json!({"entry":p.path,"sources":sources});
            } else {
                if entry.extension().is_none_or(|e| e != "ps1") {
                    return Err("Selecione um script PowerShell .ps1.".into());
                }
                let bytes = scope.bytes(&p.path)?;
                p.manifest.insert(p.path.clone(), hash(&bytes));
                p.payload = json!({"source":String::from_utf8(bytes).unwrap()});
            }
            p.kind = format!("execute:{}", a.executable);
        }
        _ => return Err("Operação desconhecida.".into()),
    }
    if reason.trim().is_empty() || reason.len() > 2000 {
        return Err("Informe um motivo curto para aprovação.".into());
    }
    if let Some(after) = &p.after {
        if after.len() > scope.max_file
            || after.contains(&0)
            || crate::memory::has_secret(
                std::str::from_utf8(after).map_err(|_| "Conteúdo deve ser UTF-8.")?,
            )
        {
            return Err("Conteúdo excede limite, é binário ou contém possível segredo.".into());
        }
    }
    let old = if p.before.is_some() {
        String::from_utf8(scope.bytes(&p.path)?).unwrap_or_default()
    } else {
        String::new()
    };
    let new = p
        .after
        .as_ref()
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_default();
    let preview = if p.after.is_some() {
        format!(
            "--- {}\n+++ {}\n{}{}",
            p.path,
            p.path,
            old.lines().map(|l| format!("-{l}\n")).collect::<String>(),
            new.lines().map(|l| format!("+{l}\n")).collect::<String>()
        )
    } else {
        String::new()
    };
    p.proposal = json!({"tool":name,"workspace":scope.root,"path":p.path,"destination":p.destination,"reason":redact(&reason),"diff":preview.chars().take(32000).collect::<String>(),"preview_truncated":preview.chars().count()>32000,"before_sha256":p.before,"after_sha256":p.after.as_ref().map(|b|hash(b)),"executable":p.exe,"arguments":if p.kind.starts_with("execute:"){vec![p.path.clone()]}else{vec![]},"cwd":p.cwd,"manifest":p.manifest,"runtime_sha256":p.exe_hash,"policy":"once; no elevation; restricted language; no Git"});
    Ok(p)
}
struct Waiting {
    sender: mpsc::SyncSender<bool>,
}
static WAITING: Mutex<Option<(String, Waiting)>> = Mutex::new(None);
static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static OWN_PROCESSES: Mutex<Vec<(String, u32, f64)>> = Mutex::new(Vec::new());

pub struct CallBudget {
    remaining: usize,
    repeats: HashMap<String, usize>,
}
impl CallBudget {
    pub fn new(max: usize) -> Self {
        Self {
            remaining: max,
            repeats: HashMap::new(),
        }
    }
    pub fn exhausted(&self) -> bool {
        self.remaining == 0
    }
    pub fn check(&mut self, name: &str, args: &str) -> Result<(), String> {
        if self.remaining == 0 {
            return Err("Limite de chamadas atingido.".into());
        }
        self.remaining -= 1;
        let key = format!("{name}:{}", hash(args.as_bytes()));
        let n = self.repeats.entry(key).or_default();
        *n += 1;
        if *n > 3 {
            return Err("Chamada repetida bloqueada; reveja o resultado anterior.".into());
        }
        Ok(())
    }
}
pub fn decide(id: &str, allow: bool) -> Result<(), String> {
    let mut w = WAITING.lock().map_err(|_| "Aprovação indisponível.")?;
    if w.as_ref().is_none_or(|(key, _)| key != id) {
        return Err("Aprovação expirada ou de outra operação.".into());
    }
    let (_, pending) = w.take().unwrap();
    pending
        .sender
        .send(allow)
        .map_err(|_| "Solicitação já encerrada.".into())
}
fn approval(
    id: &str,
    task: &str,
    proposal: &Value,
    cancel: &AtomicBool,
    event: &dyn Fn(Value),
) -> Result<bool, String> {
    let (tx, rx) = mpsc::sync_channel(1);
    {
        let mut pending = WAITING.lock().map_err(|_| "Aprovação indisponível.")?;
        if pending.is_some() {
            return Err("Outra operação aguarda aprovação.".into());
        }
        *pending = Some((id.into(), Waiting { sender: tx }));
    }
    event(json!({"state":"awaiting_approval","id":id,"task":task,"proposal":proposal}));
    let start = Instant::now();
    let answer = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err("Tarefa cancelada antes da aplicação.".into());
        }
        if start.elapsed() > Duration::from_secs(600) {
            break Err("Aprovação expirou; proponha novamente.".into());
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(v) => break Ok(v),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break Err("Aprovação encerrada.".into()),
        }
    };
    if let Ok(mut pending) = WAITING.lock() {
        if pending.as_ref().is_some_and(|(key, _)| key == id) {
            pending.take();
        }
    }
    answer
}
fn atomic(path: &Path, bytes: &[u8], replace: bool) -> Result<(), String> {
    let temp = path.with_file_name(format!(".lucille-{}.tmp", crate::controle::sufixo_unico()));
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|_| "Falha ao preparar escrita atômica.")?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| "Falha ao sincronizar alteração.")?;
    drop(f);
    let wide = |p: &Path| {
        use std::os::windows::ffi::OsStrExt;
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let flags = MOVEFILE_WRITE_THROUGH
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    let ok = unsafe { MoveFileExW(wide(&temp).as_ptr(), wide(path).as_ptr(), flags) };
    if ok == 0 {
        let _ = fs::remove_file(temp);
        return Err(
            "Não foi possível aplicar alteração atômica; confira conflito/permissões.".into(),
        );
    }
    Ok(())
}
fn apply(
    c: &Caminhos,
    scope: &Scope,
    cfg: &Config,
    id: &str,
    p: Prepared,
    cancel: &AtomicBool,
    event: &dyn Fn(Value),
) -> Result<Value, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Tarefa cancelada.".into());
    }
    let mut target = scope.path(
        &p.path,
        p.kind == "workspace_write" || p.kind == "workspace_mkdir",
    )?;
    if let Some(before) = &p.before {
        if hash(&scope.bytes(&p.path)?) != *before {
            return Err("Conflito: arquivo mudou após a proposta/aprovação.".into());
        }
        let backup = private(c)?.join(format!("{id}.backup"));
        fs::write(backup, scope.bytes(&p.path)?)
            .map_err(|_| "Não foi possível criar backup; alteração cancelada.")?;
    }
    if !p.kind.starts_with("execute:") && target.path.exists() {
        target._locks.pop();
    }
    match p.kind.as_str() {
        "workspace_write" => {
            if target.path.exists() {
                return Err("Conflito: o arquivo foi criado por outro processo.".into());
            }
            atomic(&target.path, p.after.as_ref().unwrap(), false)?;
        }
        "workspace_patch" => atomic(&target.path, p.after.as_ref().unwrap(), true)?,
        "workspace_mkdir" => {
            fs::create_dir(&target.path)
                .map_err(|_| "Não foi possível criar diretório (pai deve existir).")?;
        }
        "workspace_move" => {
            let dest = scope.path(p.destination.as_ref().unwrap(), true)?;
            if dest.path.exists() {
                return Err("Destino já existe.".into());
            }
            fs::rename(&target.path, &dest.path).map_err(|_| "Falha ao mover caminho.")?;
        }
        "workspace_delete" => {
            if target.path.is_dir() {
                fs::remove_dir(&target.path)
            } else {
                fs::remove_file(&target.path)
            }
            .map_err(|_| "Não foi possível excluir; nada externo é removido.")?;
        }
        kind if kind.starts_with("execute:") => {
            if crate::developer_runner::elevated() {
                return Err(
                    "Execução bloqueada em processo elevado. Abra o ORDIS sem administrador."
                        .into(),
                );
            }
            let exe = p.exe.as_ref().unwrap();
            if hash(&fs::read(exe).map_err(|_| "Runtime indisponível.")?) != p.exe_hash.unwrap() {
                return Err("Runtime mudou após aprovação.".into());
            }
            for (file, digest) in &p.manifest {
                if hash(&scope.bytes(file)?) != *digest {
                    return Err("Conflito: fonte aprovada mudou antes da execução.".into());
                }
            }
            let cwd = scope.path(&p.cwd, false)?;
            if !cwd.path.is_dir() {
                return Err("cwd não é diretório.".into());
            }
            let _exe_guard = guard(exe, false)?;
            return crate::developer_runner::run(
                exe,
                kind.strip_prefix("execute:").unwrap(),
                p.payload,
                &cwd.path,
                &private(c)?,
                cfg.permissions.timeout_seconds,
                cfg.max_output_bytes,
                cancel,
                event,
            );
        }
        _ => return Err("Operação inválida.".into()),
    }
    Ok(
        json!({"state":"completed","path":p.path,"sha256":p.after.as_ref().map(|b|hash(b)),"backup_created":p.before.is_some()}),
    )
}
pub fn execute(
    c: &Caminhos,
    task: &str,
    name: &str,
    args: Value,
    cancel: &AtomicBool,
    event: &dyn Fn(Value),
) -> Result<Value, String> {
    execute_with(c, task, name, args, cancel, event, &|id, proposal| {
        approval(id, task, proposal, cancel, event)
    })
}
fn execute_with(
    c: &Caminhos,
    task: &str,
    name: &str,
    args: Value,
    cancel: &AtomicBool,
    event: &dyn Fn(Value),
    ask: &dyn Fn(&str, &Value) -> Result<bool, String>,
) -> Result<Value, String> {
    let cfg = load(c);
    let scope = Scope::new(c, &cfg)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Tarefa cancelada.".into());
    }
    match name {
        "workspace_read" => {
            if !cfg.permissions.allow_read {
                return Err("Leitura não autorizada.".into());
            }
            return read(&scope, parse(args)?);
        }
        "workspace_list" => {
            let a: PathArgs = parse(args)?;
            if !cfg.permissions.allow_read {
                return Err("Leitura não autorizada.".into());
            }
            let p = scope.path(&a.path, false)?;
            let mut rows = Vec::new();
            for e in fs::read_dir(p.path)
                .map_err(|_| "Diretório indisponível.")?
                .flatten()
                .take(300)
            {
                let n = e.file_name().to_string_lossy().to_string();
                if sensitive(&n) {
                    continue;
                }
                let m = fs::symlink_metadata(e.path()).map_err(|_| "Metadados indisponíveis.")?;
                rows.push(json!({"name":n,"directory":m.is_dir(),"bytes":m.len(),"link_blocked":m.file_attributes()&FILE_ATTRIBUTE_REPARSE_POINT!=0}));
            }
            return Ok(json!({"entries":rows,"limit":300,"untrusted":true}));
        }
        "workspace_search" => {
            let a: SearchArgs = parse(args)?;
            if !cfg.permissions.allow_read || a.query.is_empty() || a.query.len() > 200 {
                return Err("Busca não autorizada ou inválida.".into());
            }
            let mut rows = Vec::new();
            for file in scope.files(&a.path)? {
                if rows.len() >= 100 {
                    break;
                }
                if a.mode == "names" {
                    if file.contains(&a.query) {
                        rows.push(json!({"path":file}));
                    }
                    continue;
                }
                let Ok(bytes) = scope.bytes(&file) else {
                    continue;
                };
                let text = String::from_utf8(bytes).unwrap();
                for (n, line) in text.lines().enumerate() {
                    let matches = if a.mode == "symbols" {
                        line.split(|c: char| !c.is_alphanumeric() && c != '_')
                            .any(|s| s == a.query)
                    } else {
                        line.contains(&a.query)
                    };
                    if matches {
                        rows.push(json!({"path":file,"line":n+1,"text":redact(&line.chars().take(300).collect::<String>())}));
                        if rows.len() >= 100 {
                            break;
                        }
                    }
                }
            }
            return Ok(json!({"matches":rows,"limit":100,"untrusted":true}));
        }
        "developer_environment" | "developer_processes" | "developer_diagnostics" => {
            let values: HashMap<String, Value> = parse(args)?;
            if !values.is_empty() {
                return Err("Esta ferramenta não aceita argumentos.".into());
            }
            return inspection(c, name);
        }
        _ => {}
    }
    if name == "developer_execute" {
        if !cfg.permissions.allow_execute {
            return Err("Execução desativada; habilite explicitamente nas configurações.".into());
        }
    } else if !cfg.permissions.allow_write {
        return Err("Escrita desativada.".into());
    }
    if REFUSED
        .lock()
        .map_err(|_| "Controle de recusa indisponível.")?
        .iter()
        .any(|v| v == task)
    {
        return Err(
            "Uma operação foi recusada neste turno. Aguarde novo pedido explícito do usuário."
                .into(),
        );
    }
    let prepared = prepare(&scope, &cfg, name, args)?;
    let id = crate::controle::sufixo_unico();
    let proposal = prepared.proposal.clone();
    journal(
        c,
        &id,
        task,
        name,
        "awaiting_approval",
        &proposal,
        &json!({}),
    )?;
    let decision = ask(&id, &proposal);
    if !matches!(decision, Ok(true)) {
        REFUSED
            .lock()
            .map_err(|_| "Controle de recusa indisponível.")?
            .push(task.to_string());
        let state = if cancel.load(Ordering::Relaxed) {
            "cancelled"
        } else {
            "refused"
        };
        journal(
            c,
            &id,
            task,
            name,
            state,
            &proposal,
            &json!({"state":state}),
        )?;
        event(json!({"state":state,"id":id}));
        return decision.map(|_| json!({"state":state,"modified":false}));
    }
    if serde_json::to_value(load(c)).ok() != serde_json::to_value(&cfg).ok() {
        return Err("Permissões/configuração mudaram; proponha novamente.".into());
    }
    journal(c, &id, task, name, "running", &proposal, &json!({}))?;
    event(json!({"state":"running","id":id,"proposal":proposal}));
    let result = apply(c, &scope, &cfg, &id, prepared, cancel, &|mut v| {
        if let Some(pid) = v["pid"].as_u64() {
            if let Ok(mut own) = OWN_PROCESSES.lock() {
                own.push((
                    scope.root.to_string_lossy().into(),
                    pid as u32,
                    crate::win::creation_time(pid as u32).unwrap_or(0.0),
                ));
            }
        }
        v["id"] = json!(id);
        event(v)
    });
    let value = match &result {
        Ok(v) => v.clone(),
        Err(e) => {
            json!({"state":if cancel.load(Ordering::Relaxed){"cancelled"}else{"failed"},"error":e})
        }
    };
    journal(
        c,
        &id,
        task,
        name,
        value["state"].as_str().unwrap_or("failed"),
        &proposal,
        &value,
    )?;
    event(json!({"state":value["state"],"id":id,"result":value}));
    result
}
fn inspection(c: &Caminhos, name: &str) -> Result<Value, String> {
    if name == "developer_environment" {
        let mut tools = Vec::new();
        for (label, exe) in [
            ("Git", "git.exe"),
            ("Python", "python.exe"),
            ("Cargo", "cargo.exe"),
            ("Rust", "rustc.exe"),
            ("Node", "node.exe"),
            ("npm", "npm.cmd"),
            ("CMake", "cmake.exe"),
            ("GCC", "gcc.exe"),
            ("C++", "cl.exe"),
            ("PowerShell", "powershell.exe"),
        ] {
            let found = std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|p| p.join(exe))
                    .find(|p| p.is_file())
            });
            tools.push(json!({"tool":label,"path":found,"installed":found.is_some()}));
        }
        return Ok(json!({"tools":tools,"installs_performed":false}));
    }
    if name == "developer_diagnostics" {
        return Ok(
            json!({"elevated_execution_blocked":crate::developer_runner::elevated(),"policy":"approval once; constrained runtimes; no administrative operations","memory":crate::memory::action(c,"technical",json!({}))?}),
        );
    }
    let workspace = Path::new(&load(c).permissions.workspace)
        .canonicalize()
        .map_err(|_| "Workspace indisponível.")?
        .to_string_lossy()
        .to_string();
    let mut own = OWN_PROCESSES
        .lock()
        .map_err(|_| "Processos indisponíveis.")?;
    own.retain(|(_, pid, start)| {
        crate::win::is_running(*pid)
            && crate::win::creation_time(*pid).is_some_and(|v| (v - start).abs() < 0.01)
    });
    Ok(
        json!({"processes":own.iter().filter(|(root,_,_)|root==&workspace).map(|(_,pid,_)|json!({"pid":pid,"scope":"child started by approved developer operation"})).collect::<Vec<_>>(),"scope":"Somente filhos iniciados pelo agente neste workspace; nenhum processo externo é alterado ou tem argumentos consultados."}),
    )
}
pub fn action(c: &Caminhos, action: &str, args: Value) -> Result<Value, String> {
    match action {
        "load" => Ok(json!(load(c))),
        "save" => {
            let cfg: Config = parse(args)?;
            let cfg = validate(c, cfg)?;
            atomic(
                &private(c)?.join("developer-config.json"),
                &serde_json::to_vec_pretty(&cfg).map_err(|_| "Configuração inválida.")?,
                true,
            )?;
            Ok(json!(true))
        }
        "history" => {
            let db = db(c)?;
            let mut q = db
                .prepare("SELECT id,owner,started FROM tasks WHERE state IN ('running','waiting')")
                .map_err(|_| "Histórico indisponível.")?;
            let rows = q
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, u32>(1)?,
                        r.get::<_, f64>(2)?,
                    ))
                })
                .map_err(|_| "Histórico indisponível.")?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Histórico inválido.")?;
            drop(q);
            for (id, pid, start) in rows {
                if !crate::win::is_running(pid)
                    || crate::win::creation_time(pid)
                        .is_none_or(|actual| (actual - start).abs() > 0.01)
                {
                    db.execute("UPDATE tasks SET state='interrupted' WHERE id=?1", [&id])
                        .map_err(|_| "Falha ao recuperar tarefa.")?;
                    db.execute("UPDATE operations SET state='interrupted' WHERE task=?1 AND state IN ('running','awaiting_approval')",[id]).map_err(|_|"Falha ao recuperar operação.")?;
                }
            }
            let mut q = db
                .prepare("SELECT id,title,state,updated FROM tasks ORDER BY updated DESC LIMIT 30")
                .map_err(|_| "Histórico indisponível.")?;
            let tasks=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"state":r.get::<_,String>(2)?,"updated":r.get::<_,i64>(3)?}))).map_err(|_|"Histórico indisponível.")?.collect::<Result<Vec<_>,_>>().map_err(|_|"Histórico indisponível.")?;
            let mut q=db.prepare("SELECT id,task,tool,state,proposal,result,updated FROM operations ORDER BY updated DESC LIMIT 100").map_err(|_|"Histórico indisponível.")?;
            let ops=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"task":r.get::<_,String>(1)?,"tool":r.get::<_,String>(2)?,"state":r.get::<_,String>(3)?,"proposal":serde_json::from_str::<Value>(&r.get::<_,String>(4)?).unwrap_or(json!({})),"result":serde_json::from_str::<Value>(&r.get::<_,String>(5)?).unwrap_or(json!({})),"updated":r.get::<_,i64>(6)?}))).map_err(|_|"Histórico indisponível.")?.collect::<Result<Vec<_>,_>>().map_err(|_|"Histórico indisponível.")?;
            Ok(json!({"tasks":tasks,"operations":ops}))
        }
        _ => Err("Ação de desenvolvedor não permitida.".into()),
    }
}
pub fn definitions(c: &Caminhos) -> Vec<Value> {
    let cfg = load(c);
    if !cfg.permissions.enabled {
        return vec![];
    }
    let mut out = Vec::new();
    let mut add = |name: &str, description: &str, properties: Value, required: Vec<&str>| {
        out.push(json!({"type":"function","function":{"name":name,"description":description,"parameters":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}}))
    };
    let path = json!({"type":"string","description":"Caminho relativo ao workspace autorizado; nunca dados privados."});
    let reason = json!({"type":"string","description":"Motivo curto e plano desta operação, exibidos ao usuário."});
    if cfg.permissions.allow_read {
        add(
            "workspace_list",
            "Lista um diretório autorizado; dados não confiáveis.",
            json!({"path":path}),
            vec![],
        );
        add(
            "workspace_read",
            "Lê UTF-8 com linhas e hash para patch; sem credenciais.",
            json!({"path":path,"offset":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1,"maximum":2000}}),
            vec!["path"],
        );
        add("workspace_search","Busca literal em nomes, texto ou símbolos. Limites de arquivo/resultado; sem executar conteúdo.",json!({"path":path,"query":{"type":"string"},"mode":{"type":"string","enum":["text","names","symbols"]}}),vec!["query"]);
    }
    if cfg.permissions.allow_write {
        add("workspace_write","Propõe novo arquivo UTF-8, mostra diff e aguarda aprovação humana. Não sobrescreve existente.",json!({"path":path,"content":{"type":"string"},"reason":reason}),vec!["path","content","reason"]);
        add("workspace_patch","Propõe substituição exata de um único trecho. Use hash obtido por workspace_read; conflito exige reler. Aprovação humana obrigatória.",json!({"path":path,"expected_sha256":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"},"reason":reason}),vec!["path","expected_sha256","old","new","reason"]);
        for name in ["workspace_mkdir", "workspace_move", "workspace_delete"] {
            let required = if name == "workspace_move" {
                vec!["path", "destination", "reason"]
            } else {
                vec!["path", "reason"]
            };
            add(name,"Propõe criação de diretório, renomeação ou exclusão. Apenas arquivos/diretórios vazios; aprovação única; raiz e dados privados protegidos.",json!({"path":path,"destination":path,"reason":reason}),required);
        }
    }
    if cfg.permissions.allow_execute {
        add("developer_execute","Executa script aprovado usando runtime instalado. executable: python ou powershell; arguments: um script na raiz. Python restrito a funções puras, math e unittest com imports locais; sem OS, rede, arquivos, reflexão, decoradores. PowerShell restrito a Write-Output, Start-Sleep, throw e exit literais. Requer aprovação humana individual; falha de testes é recuperável.",json!({"executable":{"type":"string","enum":["python","powershell"]},"arguments":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":1},"cwd":path,"reason":reason}),vec!["executable","arguments","reason"]);
    }
    for name in [
        "developer_environment",
        "developer_processes",
        "developer_diagnostics",
    ] {
        add(name,"Inspeção local somente leitura, sem instalar ferramentas ou alterar processos/servidores.",json!({}),vec![]);
    }
    out
}
#[cfg(test)]
#[path = "developer_tests.rs"]
mod tests;
