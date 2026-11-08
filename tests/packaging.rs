#![cfg(unix)]
mod support;

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use support::dist;
use tempfile::TempDir;

fn cargo_metadata() -> Value {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(dist::repo_root())
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata is JSON")
}

fn archive_entries(archive: &Path) -> Vec<String> {
    let output = Command::new("tar")
        .arg("-tzf")
        .arg(archive)
        .output()
        .expect("list archive");
    assert!(
        output.status.success(),
        "tar -tzf failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut entries: Vec<String> = String::from_utf8(output.stdout)
        .expect("tar output is UTF-8")
        .lines()
        .map(str::to_string)
        .collect();
    entries.sort();
    entries
}

fn parse_checksum_line(line: &str) -> (&str, &str) {
    assert!(line.len() > 66, "checksum line is too short: {line:?}");
    let hash = &line[..64];
    assert!(
        hash.bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "checksum is not 64 lowercase hex characters: {hash:?}"
    );
    assert_eq!(&line[64..66], "  ", "checksum must use two spaces");
    assert!(!line[66..].contains(' '), "filename must be a bare name");
    (hash, &line[66..])
}

#[test]
fn crate_is_a_single_package_with_the_release_contract() {
    let metadata = cargo_metadata();
    let packages = metadata["packages"].as_array().expect("packages array");
    assert_eq!(packages.len(), 1, "workspace must contain one package");
    assert_eq!(metadata["workspace_members"].as_array().unwrap().len(), 1);

    let package = &packages[0];
    assert_eq!(package["name"], "tcamp");
    assert_eq!(package["edition"], "2024");
    assert_eq!(package["rust_version"], "1.85");
    assert_eq!(package["license"], "Apache-2.0");

    let binaries: Vec<&Value> = package["targets"]
        .as_array()
        .expect("targets array")
        .iter()
        .filter(|target| {
            target["kind"]
                .as_array()
                .expect("target kind")
                .iter()
                .any(|kind| kind == "bin")
        })
        .collect();
    assert!(
        binaries.iter().any(|target| target["name"] == "tcamp"),
        "package must expose a tcamp binary"
    );

    let manifest = fs::read_to_string(dist::repo_root().join("Cargo.toml"))
        .expect("read Cargo.toml")
        .parse::<toml_edit::DocumentMut>()
        .expect("parse Cargo.toml");
    let declared_binaries = manifest["bin"].as_array_of_tables().expect("[[bin]] table");
    assert!(
        declared_binaries
            .iter()
            .any(|binary| binary.get("name").and_then(|name| name.as_str()) == Some("tcamp")),
        "Cargo.toml must declare [[bin]] name = \"tcamp\""
    );
    assert_eq!(manifest["profile"]["release"]["lto"].as_str(), Some("thin"));
    assert_eq!(
        manifest["profile"]["release"]["strip"].as_str(),
        Some("symbols")
    );
    assert_eq!(
        manifest["profile"]["release"]["codegen-units"].as_integer(),
        Some(1)
    );

    let gitignore =
        fs::read_to_string(dist::repo_root().join(".gitignore")).expect("read .gitignore");
    assert!(
        gitignore.lines().any(|line| line.trim() == "/target"),
        ".gitignore must ignore /target"
    );

    assert!(
        dist::repo_root().join("Cargo.lock").is_file(),
        "Cargo.lock must exist"
    );
    if dist::repo_root().join(".git").is_dir() {
        match Command::new("git")
            .arg("-C")
            .arg(dist::repo_root())
            .args(["ls-files", "--error-unmatch", "Cargo.lock"])
            .output()
        {
            Ok(output) => assert!(
                output.status.success(),
                "Cargo.lock must be committed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => panic!("run git: {error}"),
        }
    }
}

#[test]
fn package_script_builds_the_contract_archive_and_checksums() {
    let temp = TempDir::new().expect("create temp dir");
    let binary = temp.path().join("build/tcamp");
    dist::write_fake_tcamp(&binary, "0.1.0");
    let target = dist::host_target();
    let out_dir = temp.path().join("releases/latest/download");

    let output = dist::run_package_script("0.1.0", &target, &binary, &out_dir);
    assert!(
        output.status.success(),
        "package.sh failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asset = dist::asset_name("0.1.0", &target);
    let archive = out_dir.join(&asset);
    assert!(archive.is_file(), "missing archive {}", archive.display());
    assert_eq!(archive_entries(&archive), ["LICENSE", "tcamp"]);

    let extracted = temp.path().join("extracted");
    fs::create_dir_all(&extracted).expect("create extraction directory");
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&extracted)
        .status()
        .expect("extract archive");
    assert!(status.success(), "tar extraction failed");
    assert_eq!(dist::directory_entries(&extracted), ["LICENSE", "tcamp"]);
    assert_eq!(dist::file_mode(&extracted.join("tcamp")), 0o755);
    assert_eq!(dist::file_mode(&extracted.join("LICENSE")), 0o644);

    let sha256_file = out_dir.join(format!("{asset}.sha256"));
    let sha256_line = fs::read_to_string(&sha256_file).expect("read per-asset checksum");
    let (hash, name) = parse_checksum_line(sha256_line.trim_end());
    assert_eq!(name, asset);
    assert_eq!(hash, dist::sha256(&archive));

    let sums = fs::read_to_string(out_dir.join("SHA256SUMS")).expect("read SHA256SUMS");
    assert_eq!(sums, format!("{}  {}\n", dist::sha256(&archive), asset));

    assert_eq!(
        dist::directory_entries(&out_dir),
        [
            "SHA256SUMS".to_string(),
            asset.clone(),
            format!("{asset}.sha256")
        ]
    );
}

#[test]
fn merged_checksums_cover_every_archive_in_two_space_format() {
    let temp = TempDir::new().expect("create temp dir");
    let target = dist::host_target();
    let out_dir = temp.path().join("releases");

    for version in ["0.1.0", "0.1.1"] {
        let binary = temp.path().join(format!("build-{version}/tcamp"));
        dist::write_fake_tcamp(&binary, version);
        let output = dist::run_package_script(version, &target, &binary, &out_dir);
        assert!(
            output.status.success(),
            "package.sh failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let asset = dist::asset_name(version, &target);
        let archive = out_dir.join(&asset);
        let per_asset =
            fs::read_to_string(out_dir.join(format!("{asset}.sha256"))).expect("read checksum");
        let (hash, name) = parse_checksum_line(per_asset.trim_end());
        assert_eq!(name, asset);
        assert_eq!(hash, dist::sha256(&archive));
    }

    let sums = fs::read_to_string(out_dir.join("SHA256SUMS")).expect("read SHA256SUMS");
    assert!(sums.ends_with('\n'), "SHA256SUMS must end with a newline");
    let lines: Vec<&str> = sums.lines().collect();
    assert_eq!(lines.len(), 2, "one line per archive");

    let mut names = Vec::new();
    for line in lines {
        let (hash, name) = parse_checksum_line(line);
        assert_eq!(hash, dist::sha256(&out_dir.join(name)));
        assert!(
            name.ends_with(".tar.gz"),
            "only archives belong in SHA256SUMS"
        );
        names.push(name.to_string());
    }
    names.sort();
    assert_eq!(
        names,
        [
            dist::asset_name("0.1.0", &target),
            dist::asset_name("0.1.1", &target)
        ]
    );
}

fn run_release_gates(binary: &Path, archive: &Path, envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new("sh");
    command
        .arg(dist::repo_root().join("scripts/check-release-gates.sh"))
        .arg(binary)
        .arg(archive);
    for (name, value) in envs {
        command.env(name, value);
    }
    command.output().expect("run release gates")
}

fn write_slow_fake_tcamp(path: &Path) {
    fs::write(
        path,
        "#!/bin/sh\nsleep 0.05\nprintf '%s\\n' 'tcamp 0.1.0'\n",
    )
    .expect("write slow fake tcamp");
    dist::make_executable(path);
}

#[test]
fn release_gate_script_accepts_small_fast_artifacts() {
    let temp = TempDir::new().expect("create temp dir");
    let binary = temp.path().join("tcamp");
    dist::write_fake_tcamp(&binary, "0.1.0");
    let archive = temp.path().join("tcamp-0.1.0-host.tar.gz");
    fs::write(&archive, b"archive").expect("write archive");

    let output = run_release_gates(
        &binary,
        &archive,
        &[
            ("TCAMP_LATENCY_SAMPLES", "3"),
            ("TCAMP_STARTUP_P95_LIMIT_MS", "1000"),
            ("TCAMP_OFFLINE_P95_LIMIT_MS", "1000"),
        ],
    );
    assert!(
        output.status.success(),
        "release gates failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("release gates: ok"),
        "release gates must report success"
    );
}

#[test]
fn release_gate_script_rejects_oversized_binary_and_archive() {
    let temp = TempDir::new().expect("create temp dir");
    let binary = temp.path().join("tcamp");
    dist::write_fake_tcamp(&binary, "0.1.0");
    let archive = temp.path().join("tcamp-0.1.0-host.tar.gz");
    fs::write(&archive, b"archive").expect("write archive");

    let binary_output = run_release_gates(
        &binary,
        &archive,
        &[
            ("TCAMP_MAX_BINARY_BYTES", "1"),
            ("TCAMP_LATENCY_SAMPLES", "3"),
        ],
    );
    assert!(!binary_output.status.success());
    assert!(
        String::from_utf8_lossy(&binary_output.stderr).contains("binary size"),
        "oversized binary must fail the size gate: {}",
        String::from_utf8_lossy(&binary_output.stderr)
    );

    let archive_output = run_release_gates(
        &binary,
        &archive,
        &[
            ("TCAMP_MAX_ARCHIVE_BYTES", "1"),
            ("TCAMP_LATENCY_SAMPLES", "3"),
        ],
    );
    assert!(!archive_output.status.success());
    assert!(
        String::from_utf8_lossy(&archive_output.stderr).contains("archive size"),
        "oversized archive must fail the size gate: {}",
        String::from_utf8_lossy(&archive_output.stderr)
    );
}

#[test]
fn release_gate_script_rejects_slow_startup() {
    let temp = TempDir::new().expect("create temp dir");
    let binary = temp.path().join("tcamp");
    write_slow_fake_tcamp(&binary);
    let archive = temp.path().join("tcamp-0.1.0-host.tar.gz");
    fs::write(&archive, b"archive").expect("write archive");

    let output = run_release_gates(
        &binary,
        &archive,
        &[
            ("TCAMP_LATENCY_SAMPLES", "3"),
            ("TCAMP_STARTUP_P95_LIMIT_MS", "1"),
            ("TCAMP_OFFLINE_P95_LIMIT_MS", "1000"),
        ],
    );
    assert!(!output.status.success(), "slow startup must fail the gate");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--version") && stderr.contains("p95"),
        "latency failure must identify the command and p95: {stderr}"
    );
}

#[test]
fn release_scope_excludes_windows_and_tokencamp_pro_coupling() {
    let workflow = fs::read_to_string(dist::repo_root().join(".github/workflows/release.yml"))
        .expect("read release workflow");
    for forbidden in ["windows", "msvc", "i686"] {
        assert!(
            !workflow.to_ascii_lowercase().contains(forbidden),
            "release workflow must not build {forbidden} artifacts"
        );
    }

    let mut files = Vec::new();
    for directory in ["src", "scripts", ".github/workflows"] {
        collect_files(&dist::repo_root().join(directory), &mut files);
    }
    for path in files {
        let contents = fs::read_to_string(&path).expect("read scope file");
        assert!(
            !contents.contains("tokencamp-pro"),
            "{} must not couple tcamp to tokencamp-pro",
            path.display()
        );
    }

    let manifest = fs::read_to_string(dist::repo_root().join("Cargo.toml")).expect("read manifest");
    assert!(
        !manifest.contains("tokencamp-pro"),
        "Cargo.toml must not depend on tokencamp-pro"
    );
}

fn collect_files(path: &Path, files: &mut Vec<std::path::PathBuf>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("read scope directory") {
            collect_files(&entry.expect("scope entry").path(), files);
        }
        return;
    }
    files.push(path.to_path_buf());
}

#[test]
fn release_gate_script_encodes_the_contract_limits() {
    let script = fs::read_to_string(dist::repo_root().join("scripts/check-release-gates.sh"))
        .expect("read release gate script");
    for expected in [
        "15728640",
        "6291456",
        "TCAMP_STARTUP_P95_LIMIT_MS:-100",
        "TCAMP_OFFLINE_P95_LIMIT_MS:-500",
        "--version",
        "--help",
        "status",
        "doctor",
        "wc -c",
        "binary size",
        "archive size",
    ] {
        assert!(
            script.contains(expected),
            "release gate script must encode {expected:?}"
        );
    }
}
