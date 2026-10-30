//! Bounded output transport; never block a native job monitor on a stopped consumer.
use serde_json::Value;
use std::io::Write;
use std::sync::{atomic::Ordering, mpsc};
pub struct Output {
    sender: Option<mpsc::SyncSender<String>>,
    done: mpsc::Receiver<Result<(), String>>,
}
impl Output {
    pub fn new() -> Self {
        let (sender, receive) = mpsc::sync_channel::<String>(64);
        let (flushed, done) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                use std::os::windows::io::{AsRawHandle, FromRawHandle};
                use windows_sys::Win32::{
                    Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS},
                    System::{
                        Console::{GetConsoleMode, GetStdHandle, WriteConsoleW, STD_OUTPUT_HANDLE},
                        Threading::GetCurrentProcess,
                    },
                };
                let mut copied = std::ptr::null_mut();
                let ok = unsafe {
                    let process = GetCurrentProcess();
                    DuplicateHandle(
                        process,
                        GetStdHandle(STD_OUTPUT_HANDLE),
                        process,
                        &mut copied,
                        0,
                        0,
                        DUPLICATE_SAME_ACCESS,
                    )
                };
                if ok == 0 {
                    return Err("Canal JSONL indisponível.".into());
                }
                // A raw duplicate avoids holding Rust's global stdout lock if a reader stalls.
                let mut stream = unsafe { std::fs::File::from_raw_handle(copied as _) };
                for line in receive {
                    let mut mode = 0;
                    if unsafe { GetConsoleMode(stream.as_raw_handle() as _, &mut mode) } != 0 {
                        let wide: Vec<u16> = line.encode_utf16().collect();
                        let mut offset = 0;
                        while offset < wide.len() {
                            let mut written = 0;
                            if unsafe {
                                WriteConsoleW(
                                    stream.as_raw_handle() as _,
                                    wide[offset..].as_ptr() as _,
                                    (wide.len() - offset) as u32,
                                    &mut written,
                                    std::ptr::null(),
                                )
                            } == 0
                                || written == 0
                            {
                                return Err("Canal JSONL indisponível.".into());
                            }
                            offset += written as usize;
                        }
                    } else {
                        stream
                            .write_all(line.as_bytes())
                            .map_err(|_| "Canal JSONL indisponível.")?;
                    }
                }
                Ok(())
            })();
            if result.is_err() {
                crate::cli_input::CANCEL.store(true, Ordering::Relaxed);
            }
            let _ = flushed.send(result);
        });
        Self {
            sender: Some(sender),
            done,
        }
    }
    pub fn emit(&self, text: String) -> bool {
        let sent = self
            .sender
            .as_ref()
            .is_some_and(|s| s.try_send(text).is_ok());
        if !sent {
            crate::cli_input::CANCEL.store(true, Ordering::Relaxed);
        }
        sent
    }
    pub fn json(&self, value: Value) -> bool {
        self.emit(format!("{value}\n"))
    }
    pub fn finish(mut self) -> Result<(), String> {
        self.sender.take();
        self.done
            .recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| "Saída bloqueada; consulte o histórico privado.".to_string())?
    }
}
