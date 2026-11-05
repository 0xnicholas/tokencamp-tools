#![cfg(unix)]

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
pub fn host_target() -> String {
    let arch = match uname("-m").as_str() {
        "arm64" | "aarch64" => "aarch64",
        "x86_64" | "amd64" => "x86_64",
        other => panic!("unsupported test host architecture: {other}"),
    };
    let os = match uname("-s").as_str() {
        "Darwin" => "apple-darwin",
        "Linux" => "unknown-linux-musl",
        other => panic!("unsupported test host OS: {other}"),
    };
    format!("{arch}-{os}")
}

fn uname(flag: &str) -> String {
    let output = Command::new("uname").arg(flag).output().expect("run uname");
    assert!(output.status.success(), "uname {flag} failed");
    String::from_utf8(output.stdout)
        .expect("uname output is UTF-8")
        .trim()
        .to_string()
}

pub fn asset_name(version: &str, target: &str) -> String {
    format!("tcamp-{version}-{target}.tar.gz")
}

pub fn write_fake_tcamp(path: &Path, version: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fake tcamp directory");
    }
    fs::write(
        path,
        format!("#!/bin/sh\nprintf '%s\\n' 'tcamp {version}'\n"),
    )
    .expect("write fake tcamp");
    make_executable(path);
}

pub fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .expect("fake executable metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make fake executable executable");
}

pub fn file_mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .expect("file metadata")
        .permissions()
        .mode()
        & 0o777
}

pub fn directory_entries(dir: &Path) -> Vec<String> {
    let mut entries: Vec<String> = fs::read_dir(dir)
        .expect("read directory")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    entries.sort();
    entries
}

pub fn run_package_script(version: &str, target: &str, binary: &Path, out_dir: &Path) -> Output {
    Command::new("sh")
        .arg(repo_root().join("scripts/package.sh"))
        .arg(version)
        .arg(target)
        .arg(binary)
        .arg(out_dir)
        .output()
        .expect("run package.sh")
}

pub fn sha256(path: &Path) -> String {
    let output = match Command::new("sha256sum").arg(path).output() {
        Ok(output) => output,
        Err(error) if error.kind() == ErrorKind::NotFound => Command::new("shasum")
            .args(["-a", "256"])
            .arg(path)
            .output()
            .expect("run shasum"),
        Err(error) => panic!("run sha256sum: {error}"),
    };
    assert!(
        output.status.success(),
        "checksum command failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("checksum output is UTF-8")
        .split_whitespace()
        .next()
        .expect("checksum hash")
        .to_ascii_lowercase()
}

pub fn write_checksums(out_dir: &Path) -> String {
    let mut names: Vec<String> = fs::read_dir(out_dir)
        .expect("read release directory")
        .map(|entry| entry.expect("directory entry").file_name())
        .filter_map(|name| name.to_str().map(str::to_string))
        .filter(|name| name.ends_with(".tar.gz"))
        .collect();
    names.sort();

    let mut sums = String::new();
    for name in names {
        sums.push_str(&format!("{}  {}\n", sha256(&out_dir.join(&name)), name));
    }
    fs::write(out_dir.join("SHA256SUMS"), &sums).expect("write SHA256SUMS");
    sums
}
