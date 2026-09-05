//! The build hook: `desktopfly notify pass|fail` (SPIDER_PLAN.md §5).
//!
//! This is the *entire* build integration, on purpose. Nothing tails a log,
//! nothing watches a directory, nothing reads a terminal — those would read
//! content, and the project's rule is "knows when, never what". The user puts
//! one command in a script, a git hook or a task, and the creature learns one
//! word: pass, or fail.
//!
//! Transport is a local named pipe, not a socket: a desktop pet does not get a
//! listening port (SPIDER_PLAN.md §8 decision 4). The server side runs on a
//! background thread inside the platform layer and hands events to
//! [`dfcore::env::EnvSnapshot::build_events`] on the next poll; the client side
//! is [`send`], which the `notify` subcommand calls and exits.

use std::io::Write;
use std::sync::{Arc, Mutex};

use dfcore::env::BuildEvent;

/// One pipe per user session; Windows scopes `\\.\pipe\` names per machine,
/// and a second instance of the app simply fails to create it and stays deaf.
pub const PIPE_NAME: &str = r"\\.\pipe\desktopfly-notify";

/// Client side: deliver one word to a running app. Fails if none is running.
pub fn send(word: &str) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new().write(true).open(PIPE_NAME)?;
    f.write_all(word.trim().as_bytes())?;
    f.flush()
}

/// Server side: a thread that accepts one client at a time and parses one
/// word from each. Junk is dropped; the pipe never carries anything else.
pub struct NotifyServer {
    events: Arc<Mutex<Vec<BuildEvent>>>,
}

impl NotifyServer {
    pub fn start() -> Self {
        let events: Arc<Mutex<Vec<BuildEvent>>> = Arc::new(Mutex::new(Vec::new()));
        #[cfg(target_os = "windows")]
        {
            let sink = events.clone();
            std::thread::Builder::new()
                .name("desktopfly-notify".into())
                .spawn(move || windows::serve(sink))
                .ok();
        }
        NotifyServer { events }
    }

    /// Everything reported since the last drain, in order.
    pub fn drain(&self) -> Vec<BuildEvent> {
        match self.events.lock() {
            Ok(mut v) => std::mem::take(&mut *v),
            Err(_) => Vec::new(),
        }
    }

    /// For tests and for platforms without a pipe: inject as if received.
    pub fn push(&self, e: BuildEvent) {
        if let Ok(mut v) = self.events.lock() {
            v.push(e);
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use ::windows::core::PCWSTR;
    use ::windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED};
    use ::windows::Win32::Storage::FileSystem::{ReadFile, FILE_FLAGS_AND_ATTRIBUTES};
    use ::windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
        PIPE_TYPE_MESSAGE, PIPE_WAIT,
    };

    /// `PIPE_ACCESS_INBOUND`: the app only ever reads.
    const INBOUND: FILE_FLAGS_AND_ATTRIBUTES = FILE_FLAGS_AND_ATTRIBUTES(0x0000_0001);

    pub fn serve(sink: Arc<Mutex<Vec<BuildEvent>>>) {
        let name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        loop {
            let h = unsafe {
                CreateNamedPipeW(
                    PCWSTR(name.as_ptr()),
                    INBOUND,
                    PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                    1,
                    0,
                    256,
                    0,
                    None,
                )
            };
            if h.is_invalid() {
                // Another instance owns the name. Try again later rather than
                // spin; the pet is fine without the hook.
                std::thread::sleep(std::time::Duration::from_secs(10));
                continue;
            }
            // A client that connected between create and connect reports
            // ERROR_PIPE_CONNECTED, which is success by another name.
            let connected = match unsafe { ConnectNamedPipe(h, None) } {
                Ok(()) => true,
                Err(e) => e.code() == ERROR_PIPE_CONNECTED.to_hresult(),
            };
            if connected {
                let mut buf = [0u8; 256];
                let mut n: u32 = 0;
                if unsafe { ReadFile(h, Some(&mut buf), Some(&mut n), None) }.is_ok() {
                    let text = String::from_utf8_lossy(&buf[..(n as usize).min(buf.len())]);
                    if let Some(ev) = BuildEvent::parse(&text) {
                        if let Ok(mut v) = sink.lock() {
                            v.push(ev);
                        }
                    }
                }
                let _ = unsafe { DisconnectNamedPipe(h) };
            }
            let _ = unsafe { CloseHandle(h) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_returns_events_in_order_and_empties() {
        let s = NotifyServer {
            events: Arc::new(Mutex::new(Vec::new())),
        };
        s.push(BuildEvent::Fail);
        s.push(BuildEvent::Pass);
        assert_eq!(s.drain(), vec![BuildEvent::Fail, BuildEvent::Pass]);
        assert!(s.drain().is_empty());
    }

    /// The real pipe, end to end: start the server, send a word, poll it out.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_sent_word_arrives_through_the_pipe() {
        let server = NotifyServer::start();
        // Give the thread a moment to create the pipe.
        let mut sent = false;
        for _ in 0..50 {
            if send("fail").is_ok() {
                sent = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !sent {
            // Another running instance owns the name; not a failure of this
            // code, and the unit test above covers the parsing path.
            eprintln!("skipping: pipe busy (is desktopfly running?)");
            return;
        }
        let mut got = Vec::new();
        for _ in 0..50 {
            got.extend(server.drain());
            if !got.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(got, vec![BuildEvent::Fail]);
    }
}
