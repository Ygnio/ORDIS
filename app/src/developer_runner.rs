//! Approved execution uses trusted bootstraps and a constrained language, not arbitrary shell.
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
fn job(process: HANDLE) -> Result<Job, String> {
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
        info.BasicLimitInformation.ActiveProcessLimit = 1;
        info.JobMemoryLimit = 512 * 1024 * 1024;
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
    let script = match kind {
        "python" => include_str!("developer_python.py"),
        "powershell" => include_str!("developer_powershell.ps1"),
        _ => return Err("Runtime não suportado para execução nesta fase.".into()),
    };
    let extension = if kind == "python" { "py" } else { "ps1" };
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
        if kind == "python" {
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
        let job = match job(child.as_raw_handle() as HANDLE) {
            Ok(j) => j,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        event(json!({"state":"running","pid":child.id()}));
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
        let mut truncated = false;
        loop {
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
            while let Ok((stream, bytes)) = rx.try_recv() {
                let used = stdout.len() + stderr.len();
                let take = cap.saturating_sub(used).min(bytes.len());
                if take < bytes.len() {
                    truncated = true;
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
            if let Some(exit) = child
                .try_wait()
                .map_err(|_| "Falha ao consultar processo filho.")?
            {
                // Closing the job also terminates any remaining descendants before joining readers.
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
                if state == "completed" && !exit.success() {
                    state = "failed";
                }
                return Ok(
                    json!({"state":state,"exit_code":exit.code(),"stdout":crate::developer::redact(&String::from_utf8_lossy(&stdout)),"stderr":crate::developer::redact(&String::from_utf8_lossy(&stderr)),"truncated":truncated,"duration_ms":started.elapsed().as_secs_f64()*1000.0}),
                );
            }
            thread::sleep(Duration::from_millis(15));
        }
    })();
    let _ = fs::remove_file(&path);
    result
}
