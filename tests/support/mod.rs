#![allow(dead_code)]

use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tempfile::TempDir;

pub struct TestEnv {
    _temp: TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
    pub xdg_config: PathBuf,
    pub xdg_state: PathBuf,
    pub claude_config: PathBuf,
    pub pi_agent: PathBuf,
    pub kimi_home: PathBuf,
    pub bin_dir: PathBuf,
}

impl TestEnv {
    pub fn new() -> Self {
        let temp = TempDir::new().expect("create temp dir");
        let root = temp.path().to_path_buf();
        let home = root.join("home");
        let xdg_config = root.join("xdg-config");
        let xdg_state = root.join("xdg-state");
        let claude_config = root.join("claude");
        let pi_agent = root.join("pi-agent");
        let kimi_home = root.join("kimi-code");
        let bin_dir = root.join("bin");
        for dir in [
            &home,
            &xdg_config,
            &xdg_state,
            &claude_config,
            &pi_agent,
            &kimi_home,
            &bin_dir,
        ] {
            fs::create_dir_all(dir).expect("create fixture directory");
        }

        Self {
            _temp: temp,
            root,
            home,
            xdg_config,
            xdg_state,
            claude_config,
            pi_agent,
            kimi_home,
            bin_dir,
        }
    }

    pub fn command(&self) -> assert_cmd::Command {
        let mut command = assert_cmd::Command::cargo_bin("tcamp").expect("build tcamp binary");
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg_config)
            .env("XDG_STATE_HOME", &self.xdg_state)
            .env("CLAUDE_CONFIG_DIR", &self.claude_config)
            .env("PI_CODING_AGENT_DIR", &self.pi_agent)
            .env("KIMI_CODE_HOME", &self.kimi_home)
            .env("PATH", &self.bin_dir);
        command
    }

    pub fn fake_harness(&self, name: &str, version: &str) -> FakeHarness {
        FakeHarness::new(&self.bin_dir, name, version, 0)
    }

    pub fn fake_harness_with_exit_code(
        &self,
        name: &str,
        version: &str,
        exit_code: i32,
    ) -> FakeHarness {
        FakeHarness::new(&self.bin_dir, name, version, exit_code)
    }
}

pub struct FakeHarness {
    path: PathBuf,
    args_log: PathBuf,
    env_log: PathBuf,
}

impl FakeHarness {
    fn new(bin_dir: &Path, name: &str, version: &str, exit_code: i32) -> Self {
        let path = bin_dir.join(name);
        let args_log = bin_dir.join(format!("{name}.args"));
        let env_log = bin_dir.join(format!("{name}.env"));
        let version_line = format!("{name} {version}");
        let script = format!(
            "#!/bin/sh\n\
             set -eu\n\
             if [ \"${{1:-}}\" = \"--version\" ]; then\n\
               printf '%s\\n' {version}\n\
               exit 0\n\
             fi\n\
             printf '%s\\n' \"$@\" > {args}\n\
             /usr/bin/env > {env}\n\
             exit {exit_code}\n",
            version = shell_quote_str(&version_line),
            args = shell_quote_path(&args_log),
            env = shell_quote_path(&env_log),
        );
        fs::write(&path, script).expect("write fake harness");
        make_executable(&path);

        Self {
            path,
            args_log,
            env_log,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn args(&self) -> Vec<String> {
        read_lines(&self.args_log)
    }

    pub fn env_value(&self, key: &str) -> Option<String> {
        read_lines(&self.env_log)
            .into_iter()
            .find_map(|line| line.strip_prefix(&format!("{key}=")).map(str::to_string))
    }
}

fn read_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn shell_quote_path(path: &Path) -> String {
    shell_quote_str(&path.to_string_lossy())
}

fn shell_quote_str(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .expect("fake harness metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make fake harness executable");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub reason: &'static str,
    pub content_type: &'static str,
    pub body: String,
}

impl MockResponse {
    pub fn json(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason_phrase(status),
            content_type: "application/json",
            body: body.into(),
        }
    }

    pub fn empty(status: u16) -> Self {
        Self {
            status,
            reason: reason_phrase(status),
            content_type: "text/plain",
            body: String::new(),
        }
    }
}

pub struct MockServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MockServer {
    pub fn start(response: MockResponse) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock server");
        listener
            .set_nonblocking(true)
            .expect("make mock server nonblocking");
        let addr = listener.local_addr().expect("mock server address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let thread_requests = Arc::clone(&requests);
        let thread_shutdown = Arc::clone(&shutdown);
        let handle = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = handle_connection(stream, &response, &thread_requests);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            requests,
            shutdown,
            handle: Some(handle),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    pub fn address(&self) -> String {
        self.addr.to_string()
    }

    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    response: &MockResponse,
    requests: &Mutex<Vec<RecordedRequest>>,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let path = request_parts.next().unwrap_or_default().to_string();

    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    requests
        .lock()
        .expect("requests lock")
        .push(RecordedRequest {
            method,
            path,
            headers,
            body: String::from_utf8_lossy(&body).into_owned(),
        });

    let response_bytes = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.reason,
        response.content_type,
        response.body.len(),
        response.body
    );
    stream.write_all(response_bytes.as_bytes())?;
    stream.flush()
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}
