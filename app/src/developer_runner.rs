//! Approved execution uses trusted bootstraps and a constrained language, not arbitrary shell.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
    System::{
        JobObjects::*,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};
struct Job(HANDLE);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JobLimits {
    seconds: u64,
    subprocess_seconds: u64,
    memory_mib: usize,
    cpu_percent: u32,
    processes: u32,
    storage_mib: u64,
    storage_dir: std::path::PathBuf,
    cancel_file: std::path::PathBuf,
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub fn elevated() -> bool {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return true;
        }
        let mut e: TOKEN_ELEVATION = std::mem::zeroed();
        let mut size = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut e as *mut _ as _,
            std::mem::size_of_val(&e) as u32,
            &mut size,
        );
        CloseHandle(token);
        ok == 0 || e.TokenIsElevated != 0
    }
}
fn job(process: HANDLE, limits: Option<&JobLimits>) -> Result<Job, String> {
    unsafe {
        let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if h.is_null() {
            return Err("Não foi possível isolar o processo filho.".into());
        }
        let j = Job(h);
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_JOB_MEMORY;
        info.BasicLimitInformation.ActiveProcessLimit = limits.map_or(1, |l| l.processes);
        info.JobMemoryLimit = limits.map_or(512, |l| l.memory_mib) * 1024 * 1024;
        if let Some(l) = limits {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_TIME;
            info.BasicLimitInformation.PerProcessUserTimeLimit =
                (l.subprocess_seconds * 10_000_000) as i64;
        }
        if SetInformationJobObject(
            h,
            JobObjectExtendedLimitInformation,
            &info as *const _ as _,
            std::mem::size_of_val(&info) as u32,
        ) == 0
            || AssignProcessToJobObject(h, process) == 0
        {
            return Err("Job Object recusado; o código não foi liberado para execução.".into());
        }
        if let Some(l) = limits {
            let cpu = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION {
                ControlFlags: JOB_OBJECT_CPU_RATE_CONTROL_ENABLE
                    | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
                Anonymous: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION_0 {
                    CpuRate: l.cpu_percent * 100,
                },
            };
            if SetInformationJobObject(
                h,
                JobObjectCpuRateControlInformation,
                &cpu as *const _ as _,
                std::mem::size_of_val(&cpu) as u32,
            ) == 0
            {
                return Err("Job Object recusou limite de CPU; execução não liberada.".into());
            }
        }
        Ok(j)
    }
}
pub fn run(
    exe: &Path,
    kind: &str,
    payload: Value,
    cwd: &Path,
    private: &Path,
    timeout: u64,
    cap: usize,
    cancel: &AtomicBool,
    event: &dyn Fn(Value),
) -> Result<Value, String> {
    let limits = if kind == "job" {
        if elevated() {
            return Err("Jobs bloqueados em processo elevado.".into());
        }
        let l: JobLimits = serde_json::from_value(payload["run_limits"].clone())
            .map_err(|_| "Limites do job inválidos.")?;
        if !(1..=600).contains(&l.seconds)
            || !(1..=l.seconds).contains(&l.subprocess_seconds)
            || !(128..=2048).contains(&l.memory_mib)
            || !(1..=80).contains(&l.cpu_percent)
            || !(2..=16).contains(&l.processes)
            || !(16..=512).contains(&l.storage_mib)
            || !l.storage_dir.starts_with(private)
            || !l.cancel_file.starts_with(&l.storage_dir)
            || timeout != l.seconds
        {
            return Err("Política de recursos recusada.".into());
        }
        Some(l)
    } else {
        None
    };
    let script = match kind {
        "python" => include_str!("developer_python.py"),
        "powershell" => include_str!("developer_powershell.ps1"),
        "job" => include_str!("developer_job_bootstrap.py"),
        _ => return Err("Runtime não suportado para execução nesta fase.".into()),
    };
    let python = matches!(kind, "python" | "job");
    let extension = if python { "py" } else { "ps1" };
    let path = private.join(format!(
        "runner-{}.{}",
        crate::controle::sufixo_unico(),
        extension
    ));
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|_| "Falha ao preparar runner privado.")?;
    f.write_all(script.trim_start_matches('\u{feff}').as_bytes())
        .map_err(|_| "Falha ao gravar runner.")?;
    f.sync_all().map_err(|_| "Falha ao sincronizar runner.")?;
    drop(f);
    let result = (|| {
        let mut command = Command::new(exe);
        if python {
            command.args(["-I", "-S", "-B", "-X", "utf8"]);
        } else {
            command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"]);
        }
        command
            .arg(&path)
            .current_dir(cwd)
            .env_clear()
            .env(
                "SystemRoot",
                std::env::var_os("SystemRoot").ok_or("SystemRoot indisponível.")?,
            )
            .env("TEMP", private)
            .env("TMP", private)
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000);
        let started = Instant::now();
        let mut child = command
            .spawn()
            .map_err(|_| "Não foi possível iniciar o runtime configurado.")?;
        let job = match job(child.as_raw_handle() as HANDLE, limits.as_ref()) {
            Ok(j) => j,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        event(json!({"state":"running","pid":child.id()}));
        if cancel.load(Ordering::Relaxed) {
            unsafe {
                TerminateJobObject(job.0, 1);
            }
            let _ = child.wait();
            return Ok(
                json!({"state":"cancelled","exit_code":null,"stdout":"","stderr":"","truncated":false,"duration_ms":started.elapsed().as_secs_f64()*1000.0}),
            );
        }
        let (tx, rx) = mpsc::sync_channel(16);
        fn reader(
            mut pipe: impl Read + Send + 'static,
            stream: &'static str,
            tx: mpsc::SyncSender<(&'static str, Vec<u8>)>,
        ) -> thread::JoinHandle<()> {
            thread::spawn(move || {
                let mut bytes = [0u8; 1024];
                loop {
                    match pipe.read(&mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.send((stream, bytes[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
        }
        let out = reader(child.stdout.take().unwrap(), "stdout", tx.clone());
        let err = reader(child.stderr.take().unwrap(), "stderr", tx.clone());
        drop(tx);
        // The child only reads the request after successful Job Object assignment.
        let mut input = child.stdin.take().unwrap();
        let encoded = serde_json::to_vec(&payload).map_err(|_| "Payload inválido.")?;
        let writer = thread::spawn(move || {
            let _ = input.write_all(&encoded);
        });
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut state = "completed";
        let mut resource_error: Option<String> = None;
        let mut measured_storage_bytes: Option<u64> = None;
        let mut truncated = false;
        let mut observed = std::collections::HashMap::<u32, Instant>::new();
        let mut owned_pids = Vec::<u32>::new();
        let mut sampled = Instant::now();
        let mut heartbeat = Instant::now();
        loop {
            if let Some(l) = &limits {
                if l.cancel_file.is_file() {
                    cancel.store(true, Ordering::Relaxed);
                }
                if sampled.elapsed() >= Duration::from_millis(100) {
                    sampled = Instant::now();
                    match members(&job) {
                        Ok(pids) => {
                            for pid in pids {
                                if !owned_pids.contains(&pid) {
                                    owned_pids.push(pid);
                                }
                                if pid != child.id()
                                    && observed.entry(pid).or_insert_with(Instant::now).elapsed()
                                        >= Duration::from_secs(l.subprocess_seconds)
                                {
                                    state = "timeout";
                                }
                            }
                        }
                        Err(e) => {
                            state = "resource_error";
                            resource_error = Some(e);
                        }
                    }
                    match storage_bytes(&l.storage_dir, 0) {
                        Ok(n) => {
                            measured_storage_bytes = Some(n);
                            if n > l.storage_mib * 1024 * 1024 {
                                state = "storage_limit";
                            }
                        }
                        Err(e) => {
                            state = "resource_error";
                            resource_error = Some(e);
                        }
                    }
                }
                if heartbeat.elapsed() >= Duration::from_millis(250) {
                    heartbeat = Instant::now();
                    event(json!({"state":"progress","elapsed_ms":started.elapsed().as_millis()}));
                }
            }
            if cancel.load(Ordering::Relaxed) {
                state = "cancelled";
                unsafe {
                    TerminateJobObject(job.0, 1);
                }
            } else if started.elapsed() >= Duration::from_secs(timeout) {
                state = "timeout";
                unsafe {
                    TerminateJobObject(job.0, 1);
                }
            }
            for _ in 0..16 {
                let Ok((stream, bytes)) = rx.try_recv() else {
                    break;
                };
                let used = stdout.len() + stderr.len();
                let take = cap.saturating_sub(used).min(bytes.len());
                if take < bytes.len() {
                    truncated = true;
                    if limits.is_some() {
                        state = "output_limit";
                    }
                }
                let target = if stream == "stdout" {
                    &mut stdout
                } else {
                    &mut stderr
                };
                target.extend_from_slice(&bytes[..take]);
                if take > 0 {
                    let text = String::from_utf8_lossy(&bytes[..take]);
                    let text = crate::developer::redact(&text);
                    event(json!({"state":"output","stream":stream,"text":text}));
                }
            }
            if !matches!(state, "completed") {
                unsafe {
                    TerminateJobObject(job.0, 1);
                }
            }
            if let Some(exit) = child
                .try_wait()
                .map_err(|_| "Falha ao consultar processo filho.")?
            {
                // Closing the job also terminates any remaining descendants before joining readers.
                let mut stats: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
                let stats_ok = unsafe {
                    QueryInformationJobObject(
                        job.0,
                        JobObjectExtendedLimitInformation,
                        &mut stats as *mut _ as _,
                        std::mem::size_of_val(&stats) as u32,
                        std::ptr::null_mut(),
                    )
                } != 0;
                drop(job);
                let _ = writer.join();
                while let Ok((stream, bytes)) = rx.recv_timeout(Duration::from_millis(50)) {
                    let used = stdout.len() + stderr.len();
                    let take = cap.saturating_sub(used).min(bytes.len());
                    if take < bytes.len() {
                        truncated = true;
                    }
                    if stream == "stdout" {
                        stdout.extend_from_slice(&bytes[..take]);
                    } else {
                        stderr.extend_from_slice(&bytes[..take]);
                    }
                }
                drop(rx);
                let _ = out.join();
                let _ = err.join();
                if limits.is_some() && truncated && state == "completed" {
                    state = "output_limit";
                }
                if state == "completed" && !exit.success() {
                    state = "failed";
                }
                return Ok(
                    json!({"state":state,"exit_code":exit.code(),"stdout":crate::developer::redact(&String::from_utf8_lossy(&stdout)),"stderr":crate::developer::redact(&String::from_utf8_lossy(&stderr)),"truncated":truncated,"duration_ms":started.elapsed().as_secs_f64()*1000.0,"owned_pids":owned_pids,"peak_job_memory_bytes":if stats_ok{Some(stats.PeakJobMemoryUsed)}else{None},"resource_error":resource_error,"measured_storage_bytes":measured_storage_bytes}),
                );
            }
            thread::sleep(Duration::from_millis(15));
        }
    })();
    let _ = fs::remove_file(&path);
    result
}

fn members(job: &Job) -> Result<Vec<u32>, String> {
    let mut data = [0usize; 65];
    if unsafe {
        QueryInformationJobObject(
            job.0,
            JobObjectBasicProcessIdList,
            data.as_mut_ptr() as _,
            std::mem::size_of_val(&data) as u32,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err("Não foi possível verificar membros do Job Object.".into());
    }
    let count = unsafe { *((data.as_ptr() as *const u32).add(1)) } as usize;
    if count > 64 {
        return Err("Lista de processos excedeu política.".into());
    }
    Ok(data[1..1 + count].iter().map(|p| *p as u32).collect())
}
#[derive(Clone, Copy, PartialEq)]
enum ScanPhase {
    BeforeMetadata,
    BeforeDirectory,
}
fn storage_bytes(dir: &Path, depth: usize) -> Result<u64, String> {
    storage_scan(dir, depth, &mut |_, _| {})
}
fn storage_scan(
    dir: &Path,
    depth: usize,
    hook: &mut impl FnMut(&Path, ScanPhase),
) -> Result<u64, String> {
    use std::{io::ErrorKind, os::windows::fs::MetadataExt};
    if depth > 16 {
        return Err("Saída excedeu profundidade permitida.".into());
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if depth > 0 && e.kind() == ErrorKind::NotFound => return Ok(0),
        Err(e) => {
            return Err(format!(
                "Não foi possível enumerar a saída: {:?}.",
                e.kind()
            ))
        }
    };
    let mut total = 0u64;
    let mut count = 0;
    for entry in entries {
        let p = match entry {
            Ok(entry) => entry.path(),
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(format!(
                    "Não foi possível enumerar um item da saída: {:?}.",
                    e.kind()
                ))
            }
        };
        count += 1;
        if count > 4096 {
            return Err("Arquivos de saída em excesso.".into());
        }
        hook(&p, ScanPhase::BeforeMetadata);
        let m = match fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("Metadados da saída indisponíveis: {:?}.", e.kind())),
        };
        if m.file_attributes() & 0x400 != 0 {
            return Err("Redirecionamento na saída bloqueado.".into());
        }
        total = total.saturating_add(if m.is_dir() {
            hook(&p, ScanPhase::BeforeDirectory);
            storage_scan(&p, depth + 1, hook)?
        } else {
            m.len()
        });
    }
    Ok(total)
}
#[cfg(test)]
mod storage_tests {
    use super::*;
    #[test]
    fn compiler_temporary_outputs_can_disappear_at_both_scan_boundaries() {
        let root = std::env::temp_dir().join(format!(
            "ordis-storage-race-{}",
            crate::controle::sufixo_unico()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("stable"), b"12345").unwrap();
        fs::write(root.join("temporary"), b"deleted by compiler").unwrap();
        fs::create_dir(root.join("temporary-directory")).unwrap();
        let bytes = storage_scan(&root, 0, &mut |p, phase| {
            if p.file_name().is_some_and(|n| n == "temporary") && phase == ScanPhase::BeforeMetadata
            {
                fs::remove_file(p).unwrap();
            }
            if p.file_name().is_some_and(|n| n == "temporary-directory")
                && phase == ScanPhase::BeforeDirectory
            {
                fs::remove_dir(p).unwrap();
            }
        })
        .unwrap();
        assert_eq!(bytes, 5);
        assert_eq!(storage_bytes(&root, 0).unwrap(), 5);
        fs::remove_dir_all(&root).unwrap();
        // A missing job root remains a failure; only vanished nested temporary entries are benign.
        assert!(storage_bytes(&root, 0).is_err());
    }
}
