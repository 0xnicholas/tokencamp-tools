#![cfg(unix)]
mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use support::dist;
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    home: PathBuf,
    releases: PathBuf,
    tmp: PathBuf,
    target: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().expect("create temp dir");
        let root = temp.path().to_path_buf();
        let home = root.join("home");
        let releases = root.join("releases");
        let tmp = root.join("tmp");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&tmp).expect("create TMPDIR");
        let target = dist::host_target();
        Self {
            _temp: temp,
            root,
            home,
            releases,
            tmp,
            target,
        }
    }

    fn latest_dir(&self) -> PathBuf {
        self.releases.join("latest/download")
    }

    fn pinned_dir(&self, version: &str) -> PathBuf {
        self.releases.join(format!("download/v{version}"))
    }

    fn add_latest(&self, version: &str) -> PathBuf {
        self.add_release(version, &self.latest_dir())
    }

    fn add_pinned(&self, version: &str) -> PathBuf {
        self.add_release(version, &self.pinned_dir(version))
    }

    fn add_release(&self, version: &str, out_dir: &Path) -> PathBuf {
        self.add_release_with_binary(version, version, out_dir)
    }

    fn add_release_with_binary(
        &self,
        version: &str,
        binary_version: &str,
        out_dir: &Path,
    ) -> PathBuf {
        let binary = self
            .root
            .join(format!("build-{version}-{binary_version}/tcamp"));
        dist::write_fake_tcamp(&binary, binary_version);
        let output = dist::run_package_script(version, &self.target, &binary, out_dir);
        assert!(
            output.status.success(),
            "package.sh failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        out_dir.join(dist::asset_name(version, &self.target))
    }

    fn run(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let mut command = Command::new("/bin/sh");
        command
            .arg(dist::repo_root().join("install.sh"))
            .args(args)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &self.home)
            .env("TMPDIR", &self.tmp)
            .env(
                "TCAMP_RELEASES_URL",
                format!("file://{}", self.releases.display()),
            );
        for (key, value) in envs {
            command.env(key, value);
        }
        command.output().expect("run install.sh")
    }

    fn assert_no_installer_temp_left(&self) {
        let leftovers: Vec<String> = fs::read_dir(&self.tmp)
            .expect("read TMPDIR")
            .map(|entry| {
                entry
                    .expect("TMPDIR entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(
            leftovers.is_empty(),
            "installer left temporary entries: {leftovers:?}"
        );
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn combined(output: &Output) -> String {
    format!("{}\n{}", stdout(output), stderr(output))
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "install.sh failed with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout(output),
        stderr(output)
    );
}

fn installed_version(install_dir: &Path) -> String {
    let output = Command::new(install_dir.join("tcamp"))
        .arg("--version")
        .output()
        .expect("run installed tcamp");
    assert!(output.status.success(), "installed tcamp --version failed");
    String::from_utf8(output.stdout).expect("version output is UTF-8")
}

fn env_path() -> String {
    std::env::var("PATH").unwrap_or_default()
}

fn assert_exit(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "expected exit {code}\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn tamper_checksums(fixture: &Fixture) {
    let path = fixture.latest_dir().join("SHA256SUMS");
    let contents = fs::read_to_string(&path).expect("read SHA256SUMS");
    let mut lines = contents.lines();
    let first = lines.next().expect("checksum line");
    assert!(first.len() > 66, "unexpected checksum line: {first:?}");

    let replacement = if first.starts_with('0') { '1' } else { '0' };
    let mut tampered = String::new();
    tampered.push(replacement);
    tampered.push_str(&first[1..]);
    tampered.push('\n');
    for line in lines {
        tampered.push_str(line);
        tampered.push('\n');
    }
    fs::write(&path, tampered).expect("tamper SHA256SUMS");
}

fn find_on_path(command: &str, path: &str) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|directory| directory.join(command))
        .find(|candidate| candidate.is_file())
}

impl Fixture {
    fn fake_uname_dir(&self, system: &str, machine: &str) -> PathBuf {
        let directory = self.root.join(format!("uname-{system}-{machine}"));
        fs::create_dir_all(&directory).expect("create fake uname directory");
        let script = format!(
            "#!/bin/sh\ncase \"${{1:-}}\" in\n  -s) printf '%s\\n' '{system}' ;;\n  -m) printf '%s\\n' '{machine}' ;;\nesac\n"
        );
        let path = directory.join("uname");
        fs::write(&path, script).expect("write fake uname");
        dist::make_executable(&path);
        directory
    }

    fn sha_free_path(&self) -> PathBuf {
        let directory = self.root.join("sha-free-bin");
        fs::create_dir_all(&directory).expect("create sanitized PATH");
        for command in [
            "uname", "curl", "mktemp", "mkdir", "rm", "awk", "sed", "tar", "install", "mv", "cp",
            "cat",
        ] {
            if let Some(found) = find_on_path(command, &env_path()) {
                std::os::unix::fs::symlink(found, directory.join(command))
                    .expect("link command into sanitized PATH");
            }
        }
        directory
    }
}

#[test]
fn installs_latest_archive_to_requested_directory() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");
    let install_dir = fixture.root.join("install");

    let output = fixture.run(&[], &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())]);

    assert_success(&output);
    assert_eq!(installed_version(&install_dir), "tcamp 0.1.0\n");
    assert_eq!(dist::file_mode(&install_dir.join("tcamp")), 0o755);
    let hint = format!("export PATH=\"{}:$PATH\"", install_dir.display());
    assert!(
        combined(&output).contains(&hint),
        "missing PATH hint {hint:?} in: {}",
        combined(&output)
    );
    fixture.assert_no_installer_temp_left();
}

#[test]
fn defaults_to_home_local_bin_and_prints_path_hint() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");

    let output = fixture.run(&[], &[]);

    assert_success(&output);
    let install_dir = fixture.home.join(".local/bin");
    assert_eq!(installed_version(&install_dir), "tcamp 0.1.0\n");
    assert_eq!(dist::file_mode(&install_dir), 0o755);
    let text = combined(&output);
    assert!(
        text.contains(&format!("export PATH=\"{}:$PATH\"", install_dir.display())),
        "missing PATH hint in: {text}"
    );
    fixture.assert_no_installer_temp_left();
}

#[test]
fn version_flag_selects_pinned_release_and_overrides_environment() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");
    fixture.add_pinned("0.2.0");
    fixture.add_pinned("0.3.0");
    let install_dir = fixture.root.join("install");
    let install_dir_str = install_dir.to_str().unwrap();

    let output = fixture.run(
        &[],
        &[
            ("TCAMP_INSTALL_DIR", install_dir_str),
            ("TCAMP_VERSION", "v0.2.0"),
        ],
    );
    assert_success(&output);
    assert_eq!(installed_version(&install_dir), "tcamp 0.2.0\n");

    let output = fixture.run(
        &["--version", "v0.3.0"],
        &[
            ("TCAMP_INSTALL_DIR", install_dir_str),
            ("TCAMP_VERSION", "v0.2.0"),
        ],
    );
    assert_success(&output);
    assert_eq!(installed_version(&install_dir), "tcamp 0.3.0\n");
}

#[test]
fn version_equals_form_is_accepted() {
    let fixture = Fixture::new();
    fixture.add_pinned("0.4.0");
    let install_dir = fixture.root.join("install");

    let output = fixture.run(
        &["--version=0.4.0"],
        &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())],
    );

    assert_success(&output);
    assert_eq!(installed_version(&install_dir), "tcamp 0.4.0\n");
}

#[test]
fn does_not_print_path_hint_when_directory_is_already_on_path() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");
    let install_dir = fixture.root.join("install");
    fs::create_dir_all(&install_dir).expect("create install dir");
    let path = format!("{}:{}", install_dir.display(), env_path());

    let output = fixture.run(
        &[],
        &[
            ("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap()),
            ("PATH", &path),
        ],
    );

    assert_success(&output);
    assert!(
        !combined(&output).contains("export PATH="),
        "unexpected PATH hint: {}",
        combined(&output)
    );
}

#[test]
fn installer_is_valid_posix_sh() {
    let script_path = dist::repo_root().join("install.sh");
    let script = fs::read_to_string(&script_path).expect("read install.sh");

    assert!(
        script.starts_with("#!/bin/sh\n"),
        "install.sh needs a sh shebang"
    );
    assert!(script.contains("set -eu"), "install.sh needs set -eu");

    let status = Command::new("sh")
        .arg("-n")
        .arg(&script_path)
        .status()
        .expect("run sh -n");
    assert!(status.success(), "install.sh is not valid POSIX sh");

    if Command::new("shellcheck")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        let output = Command::new("shellcheck")
            .arg(&script_path)
            .output()
            .expect("run shellcheck");
        assert!(
            output.status.success(),
            "shellcheck failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn checksum_mismatch_exits_1_and_leaves_no_binary() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");
    tamper_checksums(&fixture);
    let install_dir = fixture.root.join("install");

    let output = fixture.run(&[], &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())]);

    assert_exit(&output, 1);
    assert!(
        combined(&output).contains("refusing to install"),
        "missing refusal message: {}",
        combined(&output)
    );
    assert!(!install_dir.join("tcamp").exists());
    fixture.assert_no_installer_temp_left();
}

#[test]
fn checksum_failure_preserves_an_existing_install() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");
    let install_dir = fixture.root.join("install");
    fs::create_dir_all(&install_dir).expect("create install dir");
    dist::write_fake_tcamp(&install_dir.join("tcamp"), "0.0.0");
    let before = fs::read(install_dir.join("tcamp")).expect("read existing binary");
    tamper_checksums(&fixture);

    let output = fixture.run(&[], &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())]);

    assert_exit(&output, 1);
    assert_eq!(
        fs::read(install_dir.join("tcamp")).expect("read preserved binary"),
        before,
        "existing binary was modified"
    );
    fixture.assert_no_installer_temp_left();
}

#[test]
fn missing_checksum_tool_fails_without_installing() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");
    let install_dir = fixture.root.join("install");
    let sanitized_path = fixture.sha_free_path();

    let output = fixture.run(
        &[],
        &[
            ("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap()),
            ("PATH", sanitized_path.to_str().unwrap()),
        ],
    );

    assert_exit(&output, 1);
    assert!(!install_dir.join("tcamp").exists());
    fixture.assert_no_installer_temp_left();
}

#[test]
fn unsupported_operating_system_exits_2() {
    let fixture = Fixture::new();
    let install_dir = fixture.root.join("install");
    let fake_bin = fixture.fake_uname_dir("MINGW64_NT-10.0", "x86_64");
    let path = format!("{}:{}", fake_bin.display(), env_path());

    let output = fixture.run(
        &[],
        &[
            ("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap()),
            ("PATH", &path),
        ],
    );

    assert_exit(&output, 2);
    let text = combined(&output);
    assert!(text.contains("unsupported operating system"), "{text}");
    assert!(text.contains("macOS and Linux"), "{text}");
    assert!(!install_dir.join("tcamp").exists());
}

#[test]
fn unsupported_architecture_exits_2() {
    let fixture = Fixture::new();
    let install_dir = fixture.root.join("install");
    let fake_bin = fixture.fake_uname_dir("Linux", "i686");
    let path = format!("{}:{}", fake_bin.display(), env_path());

    let output = fixture.run(
        &[],
        &[
            ("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap()),
            ("PATH", &path),
        ],
    );

    assert_exit(&output, 2);
    let text = combined(&output);
    assert!(text.contains("unsupported architecture"), "{text}");
    assert!(text.contains("arm64 and x86_64"), "{text}");
    assert!(!install_dir.join("tcamp").exists());
}

#[test]
fn unknown_argument_exits_2() {
    let fixture = Fixture::new();
    let install_dir = fixture.root.join("install");

    let output = fixture.run(
        &["--bogus"],
        &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())],
    );

    assert_exit(&output, 2);
    assert!(!install_dir.join("tcamp").exists());
}

#[test]
fn version_flag_without_value_exits_2() {
    let fixture = Fixture::new();
    let install_dir = fixture.root.join("install");

    let output = fixture.run(
        &["--version"],
        &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())],
    );

    assert_exit(&output, 2);
    assert!(!install_dir.join("tcamp").exists());
}

#[test]
fn version_mismatch_restores_the_previous_binary() {
    let fixture = Fixture::new();
    fixture.add_release_with_binary("0.1.0", "9.9.9", &fixture.latest_dir());

    let install_dir = fixture.root.join("install");
    fs::create_dir_all(&install_dir).expect("create install dir");
    dist::write_fake_tcamp(&install_dir.join("tcamp"), "0.0.0");
    let before = fs::read(install_dir.join("tcamp")).expect("read existing binary");

    let output = fixture.run(&[], &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())]);

    assert_exit(&output, 1);
    assert_eq!(
        fs::read(install_dir.join("tcamp")).expect("read restored binary"),
        before,
        "previous binary was not restored"
    );
    fixture.assert_no_installer_temp_left();

    let fresh_install = fixture.root.join("fresh-install");
    let output = fixture.run(
        &[],
        &[("TCAMP_INSTALL_DIR", fresh_install.to_str().unwrap())],
    );
    assert_exit(&output, 1);
    assert!(!fresh_install.join("tcamp").exists());
}

#[test]
fn archive_without_tcamp_is_refused() {
    let fixture = Fixture::new();
    let out_dir = fixture.latest_dir();
    fs::create_dir_all(&out_dir).expect("create latest directory");
    let staging = fixture.root.join("bad-stage");
    fs::create_dir_all(&staging).expect("create staging directory");
    fs::copy(dist::repo_root().join("LICENSE"), staging.join("LICENSE")).expect("copy LICENSE");
    let asset = dist::asset_name("0.1.0", &fixture.target);
    let status = Command::new("tar")
        .arg("-czf")
        .arg(out_dir.join(&asset))
        .arg("-C")
        .arg(&staging)
        .arg("LICENSE")
        .status()
        .expect("create malformed archive");
    assert!(status.success());
    dist::write_checksums(&out_dir);

    let install_dir = fixture.root.join("install");
    let output = fixture.run(&[], &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())]);

    assert_exit(&output, 1);
    assert!(!install_dir.join("tcamp").exists());
    fixture.assert_no_installer_temp_left();
}

#[test]
fn latest_release_without_matching_archive_is_refused() {
    let fixture = Fixture::new();
    let out_dir = fixture.latest_dir();
    fs::create_dir_all(&out_dir).expect("create latest directory");
    let other_target = if fixture.target.contains("aarch64") {
        "x86_64-unknown-linux-musl"
    } else {
        "aarch64-unknown-linux-musl"
    };
    fs::write(
        out_dir.join("SHA256SUMS"),
        format!(
            "{}  {}\n",
            "0".repeat(64),
            dist::asset_name("0.1.0", other_target)
        ),
    )
    .expect("write SHA256SUMS");

    let install_dir = fixture.root.join("install");
    let output = fixture.run(&[], &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())]);

    assert_exit(&output, 1);
    assert!(!install_dir.join("tcamp").exists());
}

#[test]
fn missing_release_metadata_fails_before_install() {
    let fixture = Fixture::new();
    let install_dir = fixture.root.join("install");

    let output = fixture.run(&[], &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())]);

    assert_exit(&output, 1);
    assert!(!install_dir.join("tcamp").exists());
    fixture.assert_no_installer_temp_left();
}

#[test]
fn install_does_not_invoke_sudo_or_dequarantine() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");
    let tool_dir = fixture.root.join("forbidden-tools");
    fs::create_dir_all(&tool_dir).expect("create fake tool directory");
    for tool in ["sudo", "xattr"] {
        let marker = tool_dir.join(format!("{tool}.called"));
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
            marker.display()
        );
        let path = tool_dir.join(tool);
        fs::write(&path, script).expect("write fake tool");
        dist::make_executable(&path);
    }
    let path = format!("{}:{}", tool_dir.display(), env_path());
    let install_dir = fixture.root.join("install");

    let output = fixture.run(
        &[],
        &[
            ("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap()),
            ("PATH", &path),
        ],
    );

    assert_success(&output);
    for tool in ["sudo", "xattr"] {
        assert!(
            !tool_dir.join(format!("{tool}.called")).exists(),
            "installer invoked {tool}"
        );
    }
}

#[test]
fn install_does_not_modify_shell_profiles_or_harness_config() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");
    let profiles = [
        (".bashrc", "bash\n"),
        (".zshrc", "zsh\n"),
        (".profile", "profile\n"),
    ];
    for (name, contents) in profiles {
        fs::write(fixture.home.join(name), contents).expect("write profile fixture");
    }
    let xdg_config = fixture.root.join("xdg-config");
    fs::create_dir_all(&xdg_config).expect("create XDG_CONFIG_HOME");
    fs::write(xdg_config.join("sentinel"), "keep").expect("write XDG sentinel");

    let output = fixture.run(&[], &[("XDG_CONFIG_HOME", xdg_config.to_str().unwrap())]);

    assert_success(&output);
    assert_eq!(
        dist::directory_entries(&fixture.home),
        vec![
            ".bashrc".to_string(),
            ".local".to_string(),
            ".profile".to_string(),
            ".zshrc".to_string()
        ]
    );
    for (name, contents) in profiles {
        assert_eq!(
            fs::read_to_string(fixture.home.join(name)).expect("read profile"),
            contents,
            "{name} was modified"
        );
    }
    assert_eq!(dist::directory_entries(&xdg_config), ["sentinel"]);
}

#[test]
fn uninstall_flag_is_rejected() {
    let fixture = Fixture::new();
    let install_dir = fixture.root.join("install");

    let output = fixture.run(
        &["--uninstall"],
        &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())],
    );

    assert_exit(&output, 2);
    assert!(!install_dir.join("tcamp").exists());
}

#[test]
fn reinstalling_the_same_version_rewrites_the_binary() {
    let fixture = Fixture::new();
    fixture.add_latest("0.1.0");
    let install_dir = fixture.root.join("install");

    let output = fixture.run(&[], &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())]);
    assert_success(&output);
    fs::write(install_dir.join("tcamp"), b"corrupted").expect("corrupt installed binary");

    let output = fixture.run(&[], &[("TCAMP_INSTALL_DIR", install_dir.to_str().unwrap())]);

    assert_success(&output);
    assert_eq!(installed_version(&install_dir), "tcamp 0.1.0\n");
    fixture.assert_no_installer_temp_left();
}
