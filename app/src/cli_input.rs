//! Human control frames come only from stdin, never from model events.
use serde::Deserialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Mutex,
};
use std::time::{Duration, Instant};

pub static CANCEL: AtomicBool = AtomicBool::new(false);
const MAX_INITIAL: usize = 32 * 1024 * 1024;
const MAX_CONTROL: u64 = 65536;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum Frame {
    #[serde(rename = "approval")]
    Approval { id: String, decision: Decision },
    #[serde(rename = "cancel")]
    Cancel,
}
#[derive(Debug, Deserialize)]
enum Decision {
    #[serde(rename = "allow")]
    Allow,
    #[serde(rename = "deny")]
    Deny,
}
struct Received {
    at: Instant,
    frame: Result<Frame, String>,
}
pub struct Input {
    receiver: Mutex<mpsc::Receiver<Received>>,
}

/// A complete object ends the first frame, without waiting for EOF or eating the next line.
fn initial<R: Read>(reader: &mut R) -> Result<Value, String> {
    let mut bytes = Vec::new();
    let (mut depth, mut string, mut escaped) = (0usize, false, false);
    loop {
        let mut b = [0];
        if reader.read(&mut b).map_err(|_| "Falha na entrada JSON.")? == 0 {
            return Err("JSON inicial incompleto.".into());
        }
        let byte = b[0];
        if bytes.is_empty() && byte.is_ascii_whitespace() {
            continue;
        }
        if bytes.is_empty() && byte != b'{' {
            return Err("A entrada inicial deve ser um objeto JSON.".into());
        }
        bytes.push(byte);
        if bytes.len() > MAX_INITIAL {
            return Err("Entrada excessiva.".into());
        }
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth = depth.checked_sub(1).ok_or("JSON inicial inválido.")?;
                    if depth == 0 {
                        return serde_json::from_slice(&bytes)
                            .map_err(|_| "JSON inicial inválido.".into());
                    }
                }
                _ => {}
            }
        }
    }
}

impl Input {
    pub fn start(json: bool) -> Result<(Self, Option<Value>), String> {
        let (tx, rx) = mpsc::sync_channel(32);
        let (first_tx, first_rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(std::io::stdin());
            if json && first_tx.send(initial(&mut reader)).is_err() {
                return;
            }
            loop {
                let mut line = Vec::new();
                let result = (&mut reader)
                    .take(MAX_CONTROL + 1)
                    .read_until(b'\n', &mut line);
                match result {
                    Ok(0) | Err(_) => break,
                    _ if line.len() as u64 > MAX_CONTROL => break, // oversize frame is a safe EOF
                    _ => {}
                }
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                let at = Instant::now();
                let frame = if !json && line.trim_ascii() == b"permitir" {
                    Ok(Frame::Approval {
                        id: String::new(),
                        decision: Decision::Allow,
                    })
                } else if !json && !line.starts_with(b"{") {
                    Ok(Frame::Approval {
                        id: String::new(),
                        decision: Decision::Deny,
                    })
                } else {
                    serde_json::from_slice(&line).map_err(|_| "Quadro de controle inválido.".into())
                };
                if matches!(frame, Ok(Frame::Cancel)) {
                    CANCEL.store(true, Ordering::Relaxed);
                }
                if tx.send(Received { at, frame }).is_err() {
                    break;
                }
            }
        });
        let first = if json {
            Some(loop {
                if CANCEL.load(Ordering::Relaxed) {
                    return Err("Entrada cancelada.".into());
                }
                match first_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(value) => break value?,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => return Err("Entrada JSON encerrada.".into()),
                }
            })
        } else {
            None
        };
        Ok((
            Self {
                receiver: Mutex::new(rx),
            },
            first,
        ))
    }

    pub fn approve(&self, id: &str, seconds: u64, legacy: bool, opened: Instant) -> bool {
        let deadline = opened + Duration::from_secs(seconds.clamp(1, 600));
        let Ok(rx) = self.receiver.lock() else {
            return false;
        };
        loop {
            if CANCEL.load(Ordering::Relaxed) || Instant::now() >= deadline {
                return false;
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(message) => match validate(message, id, opened, deadline, legacy) {
                    Ok(Some(allow)) => return allow,
                    Ok(None) => return false,
                    Err(e) => eprintln!("[aprovação recusada] {e}"),
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return false,
            }
        }
    }
}

fn validate(
    message: Received,
    id: &str,
    opened: Instant,
    deadline: Instant,
    legacy: bool,
) -> Result<Option<bool>, String> {
    if message.at < opened || message.at >= deadline {
        return Err("Decisão antecipada ou expirada.".into());
    }
    match message.frame? {
        Frame::Approval {
            id: supplied,
            decision,
        } if supplied == id || (legacy && supplied.is_empty()) => {
            Ok(Some(matches!(decision, Decision::Allow)))
        }
        Frame::Approval { .. } => Err("Identificador de outra operação ou já encerrado.".into()),
        Frame::Cancel => Ok(None),
    }
}

unsafe extern "system" fn console_control(event: u32) -> i32 {
    if event == 0 || event == 1 {
        CANCEL.store(true, Ordering::Relaxed);
        1
    } else {
        0
    }
}
pub fn install_cancel_handler() -> Result<(), String> {
    let mut member = 0u32;
    if unsafe { windows_sys::Win32::System::Console::GetConsoleProcessList(&mut member, 1) } == 0 {
        // GUI hosts and pipe-only callers may deliberately have no console to signal.
        if unsafe { windows_sys::Win32::Foundation::GetLastError() }
            == windows_sys::Win32::Foundation::ERROR_INVALID_HANDLE
        {
            return Ok(());
        }
        return Err("Não foi possível verificar o console da CLI.".into());
    }
    // Clear only this process's inherited ignore flag (some pipe/GUI launchers set it).
    if unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 0) } == 0 {
        return Err("Não foi possível habilitar Ctrl+C neste console.".into());
    }
    if unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(console_control), 1)
    } == 0
    {
        return Err("Não foi possível registrar Ctrl+C seguro.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn object_framing_preserves_controls_and_pretty_json() {
        let mut r = std::io::Cursor::new(
            b"{\n\"text\":\"}\\\"[\",\"attachments\":[]}\n{\"type\":\"cancel\"}\n",
        );
        assert_eq!(initial(&mut r).unwrap()["text"], "}\"[");
        let mut rest = String::new();
        r.read_to_string(&mut rest).unwrap();
        assert_eq!(rest.trim(), r#"{"type":"cancel"}"#);
        assert!(initial(&mut std::io::Cursor::new(b"{\"text\":")).is_err());
    }
    #[test]
    fn approval_is_bound_to_id_time_and_human_channel() {
        let now = Instant::now();
        let end = now + Duration::from_secs(1);
        for (id, at) in [
            ("wrong", now),
            ("op", now - Duration::from_secs(1)),
            ("op", end),
        ] {
            assert!(validate(
                Received {
                    at,
                    frame: Ok(Frame::Approval {
                        id: id.into(),
                        decision: Decision::Allow
                    })
                },
                "op",
                now,
                end,
                false
            )
            .is_err());
        }
        let deny = Received {
            at: now,
            frame: Ok(Frame::Approval {
                id: "op".into(),
                decision: Decision::Deny,
            }),
        };
        assert_eq!(validate(deny, "op", now, end, false).unwrap(), Some(false));
        assert!(serde_json::from_str::<Frame>(
            r#"{"type":"approval","id":"op","decision":"permitir"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<Frame>(
            r#"{"type":"approval","id":"op","decision":"allow","model":true}"#
        )
        .is_err());
    }
}
