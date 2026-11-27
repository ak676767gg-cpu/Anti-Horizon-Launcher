// The console: what a running game prints, shown live in the launcher and saved to the instance's logs folder.
// Minecraft's own logging config prints XML events (meant for launchers to read); those become plain lines here.
use serde_json::json;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// How many lines the console keeps per instance; older ones are still in the log file.
const KEEP: usize = 5000;

static LOGS: Mutex<Vec<(String, VecDeque<String>)>> = Mutex::new(Vec::new());

fn with_log<R>(id: &str, f: impl FnOnce(&mut VecDeque<String>) -> R) -> R {
    let mut logs = LOGS.lock().unwrap();
    if let Some(i) = logs.iter().position(|(x, _)| x == id) {
        return f(&mut logs[i].1);
    }
    logs.push((id.to_string(), VecDeque::new()));
    let last = logs.len() - 1;
    f(&mut logs[last].1)
}

/// Starts a fresh console for an instance that is about to launch.
pub fn reset(app: &AppHandle, id: &str) {
    with_log(id, VecDeque::clear);
    let _ = app.emit("game-log", json!({ "id": id, "reset": true, "lines": [] }));
}

/// Adds the launcher's own notes (what it is doing) to the console.
pub fn note(app: &AppHandle, id: &str, line: &str) {
    push(app, id, vec![format!("[{}] [Launcher] {line}", clock())]);
}

fn push(app: &AppHandle, id: &str, lines: Vec<String>) {
    if lines.is_empty() {
        return;
    }
    with_log(id, |log| {
        log.extend(lines.iter().cloned());
        while log.len() > KEEP {
            log.pop_front();
        }
    });
    let _ = app.emit("game-log", json!({ "id": id, "lines": lines }));
}

#[tauri::command]
pub fn get_game_log(id: String) -> Vec<String> {
    with_log(&id, |log| log.iter().cloned().collect())
}

/// Reads a process's output on background threads. Every line goes to `file` and, a few times a second, to the console.
pub fn pipe(app: &AppHandle, id: &str, child: &mut std::process::Child, file: File) {
    let (tx, rx) = mpsc::channel::<String>();
    let readers: Vec<Box<dyn Read + Send>> = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    for reader in readers {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut events = XmlEvents::default();
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf);
                        for out in events.feed(line.trim_end_matches(['\r', '\n'])) {
                            if tx.send(out).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
    }
    drop(tx);
    let (app, id) = (app.clone(), id.to_string());
    std::thread::spawn(move || {
        let mut file = file;
        let mut batch: Vec<String> = Vec::new();
        loop {
            let done = match rx.recv_timeout(Duration::from_millis(120)) {
                Ok(line) => {
                    let _ = writeln!(file, "{line}");
                    batch.push(line);
                    if batch.len() < 500 {
                        continue;
                    }
                    false
                }
                Err(RecvTimeoutError::Timeout) => false,
                Err(RecvTimeoutError::Disconnected) => true,
            };
            push(&app, &id, std::mem::take(&mut batch));
            if done {
                let _ = file.flush();
                break;
            }
        }
    });
}

/// Turns Minecraft's XML log events into "[time] [thread/LEVEL]: message" lines. Anything else passes through.
#[derive(Default)]
struct XmlEvents {
    event: Option<String>,
}

impl XmlEvents {
    fn feed(&mut self, line: &str) -> Vec<String> {
        if let Some(event) = &mut self.event {
            event.push('\n');
            event.push_str(line);
        } else if line.trim_start().starts_with("<log4j:Event") {
            self.event = Some(line.to_string());
        } else {
            return if line.trim().is_empty() { Vec::new() } else { vec![line.to_string()] };
        }
        let finished = self.event.as_deref().is_some_and(|e| e.contains("</log4j:Event>"));
        if !finished {
            // A runaway event (no closing tag) is shown as it is rather than held forever.
            if self.event.as_ref().is_some_and(|e| e.len() > 200_000) {
                return self.event.take().map(|e| e.lines().map(String::from).collect()).unwrap_or_default();
            }
            return Vec::new();
        }
        let event = self.event.take().unwrap_or_default();
        let level = attr(&event, "level").unwrap_or("INFO");
        let thread = attr(&event, "thread").unwrap_or("main");
        let mut out: Vec<String> = Vec::new();
        let message = cdata(&event, "log4j:Message").unwrap_or_default();
        let mut lines = message.lines();
        out.push(format!("[{}] [{thread}/{level}]: {}", clock(), lines.next().unwrap_or_default()));
        out.extend(lines.map(String::from));
        if let Some(trace) = cdata(&event, "log4j:Throwable") {
            out.extend(trace.lines().filter(|l| !l.trim().is_empty()).map(String::from));
        }
        out
    }
}

fn attr<'a>(event: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = event.find(&key)? + key.len();
    let len = event[start..].find('"')?;
    Some(&event[start..start + len])
}

fn cdata(event: &str, tag: &str) -> Option<String> {
    let open = event.find(&format!("<{tag}>"))?;
    let rest = &event[open..];
    let start = rest.find("<![CDATA[")? + "<![CDATA[".len();
    let end = rest[start..].find("]]>")?;
    Some(rest[start..start + end].to_string())
}

/// The local time as HH:MM:SS.
fn clock() -> String {
    #[cfg(windows)]
    {
        // SAFETY: GetLocalTime only writes the SYSTEMTIME it is given.
        let t = unsafe {
            let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = std::mem::zeroed();
            windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
            t
        };
        format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond)
    }
    #[cfg(not(windows))]
    {
        let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        format!("{:02}:{:02}:{:02}", s / 3600 % 24, s / 60 % 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::XmlEvents;

    #[test]
    fn xml_events_become_lines() {
        let mut x = XmlEvents::default();
        assert!(x.feed(r#"<log4j:Event logger="net.minecraft.client.Minecraft" timestamp="1" level="WARN" thread="Render thread">"#).is_empty());
        assert!(x.feed("  <log4j:Message><![CDATA[first line").is_empty());
        assert!(x.feed("second line]]></log4j:Message>").is_empty());
        let out = x.feed("</log4j:Event>");
        assert_eq!(out.len(), 2);
        assert!(out[0].ends_with("[Render thread/WARN]: first line"), "{}", out[0]);
        assert_eq!(out[1], "second line");
        assert_eq!(x.feed("[12:00:00] [main/INFO] plain"), vec!["[12:00:00] [main/INFO] plain".to_string()]);
        assert!(x.feed("   ").is_empty());
    }

    #[test]
    fn throwables_are_kept() {
        let mut x = XmlEvents::default();
        let one = r#"<log4j:Event logger="a" timestamp="1" level="ERROR" thread="main"><log4j:Message><![CDATA[Crashed]]></log4j:Message><log4j:Throwable><![CDATA[java.lang.Boom
	at a.b(C.java:1)
]]></log4j:Throwable></log4j:Event>"#;
        let mut out = Vec::new();
        for l in one.lines() {
            out.extend(x.feed(l));
        }
        assert_eq!(out.len(), 3, "{out:?}");
        assert!(out[0].ends_with("[main/ERROR]: Crashed"));
        assert_eq!(out[1], "java.lang.Boom");
        assert_eq!(out[2], "\tat a.b(C.java:1)");
    }
}
