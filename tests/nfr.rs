mod support;

use std::fs;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use support::{MockResponse, MockServer, TestEnv};

const NETWORK_MODULES: [&str; 4] = [
    "src/http.rs",
    "src/credentials/probe.rs",
    "src/doctor/net.rs",
    "src/harness/preflight.rs",
];

struct DeadProxy {
    addr: SocketAddr,
    connections: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl DeadProxy {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind dead proxy");
        listener
            .set_nonblocking(true)
            .expect("make dead proxy nonblocking");
        let addr = listener.local_addr().expect("dead proxy address");
        let connections = Arc::new(AtomicUsize::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));

        let thread_connections = Arc::clone(&connections);
        let thread_shutdown = Arc::clone(&shutdown);
        let handle = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        thread_connections.fetch_add(1, Ordering::Relaxed);
                        drop(stream);
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
            connections,
            shutdown,
            handle: Some(handle),
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn count(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }
}

impl Drop for DeadProxy {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_through_proxy(env: &TestEnv, proxy: &str, args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = env.command();
    command
        .args(args)
        .env("HTTP_PROXY", proxy)
        .env("HTTPS_PROXY", proxy)
        .env("ALL_PROXY", proxy)
        .env("http_proxy", proxy)
        .env("https_proxy", proxy)
        .env("all_proxy", proxy)
        .env("NO_PROXY", "")
        .env("no_proxy", "");
    if let Some(input) = stdin {
        command.write_stdin(input);
    }
    command.output().expect("run tcamp")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_code(output: &Output, expected: &[i32]) {
    let code = output.status.code().expect("tcamp exited");
    assert!(
        expected.contains(&code),
        "unexpected exit code {code}; output:\n{}",
        combined(output)
    );
}

#[test]
fn offline_commands_make_zero_outbound_connections() {
    let proxy = DeadProxy::start();
    let proxy_url = proxy.url();
    let env = TestEnv::new();
    let server = MockServer::start(MockResponse::json(200, "{}"));

    let version = run_through_proxy(&env, &proxy_url, &["--version"], None);
    assert_code(&version, &[0]);
    assert!(combined(&version).contains("tcamp 0.1.0"));

    let help = run_through_proxy(&env, &proxy_url, &["--help"], None);
    assert_code(&help, &[0]);
    assert!(combined(&help).contains("Usage: tcamp"));

    let status = run_through_proxy(&env, &proxy_url, &["status"], None);
    assert_code(&status, &[0]);
    assert!(combined(&status).contains("API key: none"));

    let doctor = run_through_proxy(&env, &proxy_url, &["doctor"], None);
    assert_code(&doctor, &[0, 1]);
    assert!(combined(&doctor).contains("tcamp doctor"));

    let login = run_through_proxy(
        &env,
        &proxy_url,
        &[
            "login",
            "--key-stdin",
            "--no-verify",
            "--base-url",
            &server.url(""),
        ],
        Some("sk-0123456789abcdef0123456789abcdef"),
    );
    assert_code(&login, &[0]);
    assert!(combined(&login).contains("sk-…cdef"));

    let status_with_key = run_through_proxy(&env, &proxy_url, &["status"], None);
    assert_code(&status_with_key, &[0]);
    assert!(combined(&status_with_key).contains("sk-…cdef"));

    let doctor_with_key = run_through_proxy(&env, &proxy_url, &["doctor"], None);
    assert_code(&doctor_with_key, &[0, 1]);
    assert!(combined(&doctor_with_key).contains("sk-…cdef"));

    let logout = run_through_proxy(&env, &proxy_url, &["logout", "--all"], None);
    assert_code(&logout, &[0]);

    let uninstall = run_through_proxy(&env, &proxy_url, &["uninstall", "--dry-run"], None);
    assert_code(&uninstall, &[0]);

    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        proxy.count(),
        0,
        "an offline command connected through the dead proxy"
    );
    assert_eq!(
        server.requests().len(),
        0,
        "an offline command sent a request to the configured base URL"
    );
}

#[test]
fn dead_proxy_counts_a_real_network_attempt() {
    let proxy = DeadProxy::start();
    let env = TestEnv::new();
    let output = run_through_proxy(
        &env,
        &proxy.url(),
        &[
            "login",
            "--key-stdin",
            "--base-url",
            "https://offline.invalid",
        ],
        Some("sk-0123456789abcdef0123456789abcdef"),
    );

    assert_code(&output, &[1]);
    thread::sleep(Duration::from_millis(100));
    assert!(
        proxy.count() >= 1,
        "the dead proxy must observe a command that performs verification"
    );
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut sources = Vec::new();
    walk(&root.join("src"), &root, &mut sources);
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    sources
}

fn walk(path: &Path, root: &Path, sources: &mut Vec<(String, String)>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("read source directory") {
            walk(&entry.expect("source entry").path(), root, sources);
        }
        return;
    }
    if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
        let relative = path
            .strip_prefix(root)
            .expect("source under repository root")
            .to_string_lossy()
            .replace('\\', "/");
        sources.push((relative, fs::read_to_string(path).expect("read source")));
    }
}

fn workflow_sources() -> Vec<(String, String)> {
    let root = repo_root().join(".github/workflows");
    let mut sources = Vec::new();
    for entry in fs::read_dir(&root).expect("read workflows") {
        let path = entry.expect("workflow entry").path();
        let name = path
            .file_name()
            .expect("workflow name")
            .to_string_lossy()
            .into_owned();
        sources.push((name, fs::read_to_string(path).expect("read workflow")));
    }
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    sources
}

#[test]
fn network_client_construction_is_confined_to_the_network_modules() {
    let mut saw_client_builder = false;
    for (path, contents) in rust_sources() {
        let allowed = NETWORK_MODULES.contains(&path.as_str());
        if path == "src/http.rs" {
            saw_client_builder = contents.contains("Client::builder()");
        }
        if !allowed {
            assert!(
                !contents.contains("reqwest::"),
                "{path} must not use reqwest; network access belongs in {NETWORK_MODULES:?}"
            );
            assert!(
                !contents.contains("TcpStream::connect")
                    && !contents.contains("TcpListener")
                    && !contents.contains("UdpSocket"),
                "{path} must not open sockets directly"
            );
        }
    }
    assert!(
        saw_client_builder,
        "src/http.rs must own the single reqwest client builder"
    );
}

#[test]
fn sources_contain_no_telemetry_update_check_or_analytics_code() {
    let forbidden = [
        "telemetry",
        "analytics",
        "posthog",
        "sentry",
        "mixpanel",
        "amplitude",
        "matomo",
        "segment.io",
        "update_check",
        "update-check",
        "check_for_updates",
        "check-for-updates",
        "auto_update",
        "auto-update",
        "latest_version",
        "latest-version",
    ];
    let mut files = rust_sources();
    files.push((
        "Cargo.toml".to_string(),
        fs::read_to_string(repo_root().join("Cargo.toml")).expect("read Cargo.toml"),
    ));
    files.extend(workflow_sources());

    for (path, contents) in files {
        let lower = contents.to_ascii_lowercase();
        for needle in forbidden {
            assert!(
                !lower.contains(needle),
                "{path} contains forbidden telemetry/update marker {needle:?}"
            );
        }
    }
}

#[test]
fn dependencies_keep_the_v1_security_and_scope_boundary() {
    let manifest = fs::read_to_string(repo_root().join("Cargo.toml")).expect("read Cargo.toml");
    for forbidden in [
        "keyring",
        "openssl",
        "tokio",
        "anyhow",
        "ureq",
        "curl",
        "reqwest-middleware",
    ] {
        assert!(
            !manifest.to_ascii_lowercase().contains(forbidden),
            "Cargo.toml must not depend on {forbidden}"
        );
    }
    assert!(
        !manifest.contains("path = \"../tokencamp-pro"),
        "tcamp must not depend on the tokencamp-pro repository"
    );
}
