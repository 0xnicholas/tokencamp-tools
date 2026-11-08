mod support;

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use tempfile::TempDir;

const WARM_RUNS: usize = 1;
const SAMPLES: usize = 20;
const STARTUP_P95_MS: f64 = 100.0;
const OFFLINE_P95_MS: f64 = 500.0;
const MAX_BINARY_BYTES: u64 = 15 * 1024 * 1024;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn release_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("TCAMP_PERF_BIN") {
        return Some(PathBuf::from(path));
    }
    let path = repo_root().join("target/release/tcamp");
    path.is_file().then_some(path)
}

struct PerfEnv {
    _temp: TempDir,
    binary: PathBuf,
    home: PathBuf,
    config: PathBuf,
    state: PathBuf,
    claude: PathBuf,
    pi: PathBuf,
    kimi: PathBuf,
    bin: PathBuf,
}

impl PerfEnv {
    fn new(binary: PathBuf) -> Self {
        let temp = TempDir::new().expect("create perf temp dir");
        let root = temp.path().to_path_buf();
        let home = root.join("home");
        let config = root.join("config");
        let state = root.join("state");
        let claude = root.join("claude");
        let pi = root.join("pi");
        let kimi = root.join("kimi");
        let bin = root.join("bin");
        for directory in [&home, &config, &state, &claude, &pi, &kimi, &bin] {
            fs::create_dir_all(directory).expect("create perf fixture directory");
        }
        Self {
            _temp: temp,
            binary,
            home,
            config,
            state,
            claude,
            pi,
            kimi,
            bin,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.config)
            .env("XDG_STATE_HOME", &self.state)
            .env("CLAUDE_CONFIG_DIR", &self.claude)
            .env("PI_CODING_AGENT_DIR", &self.pi)
            .env("KIMI_CODE_HOME", &self.kimi)
            .env("PATH", &self.bin)
            .env("NO_COLOR", "1")
            .env("TERM", "dumb");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command()
            .args(args)
            .output()
            .expect("run release tcamp")
    }
}

fn p95(samples: &[Duration]) -> f64 {
    assert!(!samples.is_empty(), "no latency samples");
    let mut milliseconds: Vec<f64> = samples
        .iter()
        .map(|sample| sample.as_secs_f64() * 1000.0)
        .collect();
    milliseconds.sort_by(|left, right| left.total_cmp(right));
    let index = ((0.95 * milliseconds.len() as f64).ceil() as usize).saturating_sub(1);
    milliseconds[index]
}

fn assert_latency(env: &PerfEnv, args: &[&str], expected_codes: &[i32], limit_ms: f64) -> f64 {
    for _ in 0..WARM_RUNS {
        let warm = env.run(args);
        assert!(
            expected_codes.contains(&warm.status.code().unwrap_or(-1)),
            "warm-up {:?} exited with {:?}",
            args,
            warm.status.code()
        );
    }

    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let output = env.run(args);
        let elapsed = started.elapsed();
        assert!(
            expected_codes.contains(&output.status.code().unwrap_or(-1)),
            "{args:?} exited with {:?}; stderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        samples.push(elapsed);
    }

    let measured = p95(&samples);
    assert!(
        measured <= limit_ms,
        "{args:?} p95 {measured:.1}ms exceeds {limit_ms:.0}ms; samples: {:?}",
        samples
            .iter()
            .map(|sample| format!("{:.1}ms", sample.as_secs_f64() * 1000.0))
            .collect::<Vec<_>>()
    );
    measured
}

#[test]
#[ignore = "release-only latency gate; run with `cargo test --test perf -- --ignored`"]
fn release_binary_meets_startup_and_offline_latency_gates() {
    let Some(binary) = release_binary() else {
        eprintln!(
            "skipping perf test: no release binary at target/release/tcamp or TCAMP_PERF_BIN"
        );
        return;
    };
    assert!(
        binary.is_file(),
        "TCAMP_PERF_BIN does not point to a file: {}",
        binary.display()
    );

    let size = fs::metadata(&binary)
        .expect("release binary metadata")
        .len();
    assert!(
        size <= MAX_BINARY_BYTES,
        "release binary is {size} bytes; limit is {MAX_BINARY_BYTES} bytes"
    );

    let env = PerfEnv::new(binary);
    let version = assert_latency(&env, &["--version"], &[0], STARTUP_P95_MS);
    let help = assert_latency(&env, &["--help"], &[0], STARTUP_P95_MS);
    let status = assert_latency(&env, &["status"], &[0], OFFLINE_P95_MS);
    let doctor = assert_latency(&env, &["doctor"], &[0, 1], OFFLINE_P95_MS);

    eprintln!(
        "perf: --version p95 {version:.1}ms, --help p95 {help:.1}ms, status p95 {status:.1}ms, doctor p95 {doctor:.1}ms"
    );
}

#[test]
fn release_workflow_enforces_latency_and_size_gates() {
    let workflow = fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .expect("read release workflow");
    assert!(
        workflow.contains("scripts/check-release-gates.sh"),
        "release workflow must run the release gate script"
    );
    assert!(
        workflow.contains("scripts/package.sh"),
        "release workflow must build the packaged archive the gate measures"
    );
    assert!(
        !workflow.contains("continue-on-error"),
        "release gates must not be allowed to fail softly"
    );

    let script = fs::read_to_string(repo_root().join("scripts/check-release-gates.sh"))
        .expect("read release gate script");
    for needle in [
        "TCAMP_STARTUP_P95_LIMIT_MS:-100",
        "TCAMP_OFFLINE_P95_LIMIT_MS:-500",
        "15728640",
        "6291456",
        "--version",
        "--help",
        "status",
        "doctor",
    ] {
        assert!(
            script.contains(needle),
            "release gate script must encode {needle:?}"
        );
    }
}

#[test]
fn release_target_list_has_no_windows_or_other_platform_artifacts() {
    let workflow = fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .expect("read release workflow");
    for target in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
    ] {
        assert!(
            workflow.contains(target),
            "release workflow must build {target}"
        );
    }
    for forbidden in ["windows", "msvc", "i686", "x86_64-pc-windows"] {
        assert!(
            !workflow.to_ascii_lowercase().contains(forbidden),
            "release workflow must not build Windows or 32-bit artifacts: {forbidden}"
        );
    }
}
