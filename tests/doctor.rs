mod support;

use std::fs;
use std::path::Path;
use std::process::Output;

use support::TestEnv;

const KEY: &str = "sk-0123456789abcdef0123456789abcdef";

fn run(env: &TestEnv, args: &[&str]) -> Output {
    env.command().args(args).output().expect("run tcamp")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

fn assert_empty(path: &Path) {
    let entries: Vec<_> = fs::read_dir(path)
        .expect("read fixture directory")
        .map(|entry| entry.expect("read entry").file_name())
        .collect();
    assert!(
        entries.is_empty(),
        "expected {} to stay empty, got {entries:?}",
        path.display()
    );
}

#[test]
fn doctor_reports_an_offline_state_board_and_exits_one_without_a_key() {
    let env = TestEnv::new();

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    for expected in [
        "Connection",
        "credential.file",
        "credential.key",
        "connection.base_url",
        "Network",
        "network.reachable",
        "network.auth",
        "Harnesses",
        "claude",
        "pi",
        "kimi",
        "Summary:",
    ] {
        assert!(
            text.contains(expected),
            "missing {expected:?} in board: {text}"
        );
    }
    assert!(
        text.contains("fail  credential.key"),
        "missing key must fail: {text}"
    );
    assert!(
        text.contains("skip  network.reachable"),
        "offline network checks must skip: {text}"
    );
    assert!(stderr(&output).is_empty(), "stderr: {}", stderr(&output));

    for dir in [
        &env.home,
        &env.xdg_config,
        &env.xdg_state,
        &env.claude_config,
        &env.pi_agent,
        &env.kimi_home,
    ] {
        assert_empty(dir);
    }
}

#[test]
fn an_environment_key_reports_the_missing_file_as_a_warning() {
    let env = TestEnv::new();

    let output = env
        .command()
        .arg("doctor")
        .env("TOKENCAMP_API_KEY", KEY)
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("warn  credential.file"), "board: {text}");
    assert!(text.contains("pass  credential.key"), "board: {text}");
    assert!(!text.contains(KEY), "full API key leaked: {text}");
}

#[cfg(unix)]
#[test]
fn credential_file_and_directory_permission_problems_are_reported() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    let file = env.credentials_path();
    let directory = file.parent().expect("credentials directory").to_path_buf();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644))
        .expect("make credentials file group-readable");

    let output = run(&env, &["doctor"]);
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  credential.file"), "board: {text}");
    assert!(text.contains("0600"), "board: {text}");
    assert!(!text.contains(KEY), "full API key leaked: {text}");

    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).expect("restore file mode");
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))
        .expect("make credentials directory group-readable");

    let output = run(&env, &["doctor"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("warn  credential.file"), "board: {text}");
    assert!(text.contains("0700"), "board: {text}");
    assert!(!text.contains(KEY), "full API key leaked: {text}");
}

#[test]
fn corrupt_credentials_and_unsupported_versions_fail_without_being_overwritten() {
    for contents in ["not toml [", "version = 2\n"] {
        let env = TestEnv::new();
        let path = env.credentials_path();
        fs::create_dir_all(path.parent().expect("credentials directory"))
            .expect("create credentials directory");
        fs::write(&path, contents).expect("write credentials");

        let output = run(&env, &["doctor"]);

        assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
        let text = stdout(&output);
        assert!(text.contains("fail  credential.file"), "board: {text}");
        assert!(
            text.contains(&path.display().to_string()),
            "board must name the credentials path: {text}"
        );
        assert_eq!(
            fs::read_to_string(&path).expect("read credentials"),
            contents,
            "doctor must not overwrite the credentials file"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_symlinked_credentials_file_fails() {
    let env = TestEnv::new();
    let real = env.root.join("real-credentials.toml");
    fs::write(&real, "version = 1\n").expect("write real credentials");
    let path = env.credentials_path();
    fs::create_dir_all(path.parent().expect("credentials directory"))
        .expect("create credentials directory");
    std::os::unix::fs::symlink(&real, &path).expect("symlink credentials");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  credential.file"), "board: {text}");
    assert!(
        text.contains("symlink"),
        "board must explain the symlink problem: {text}"
    );
}

#[test]
fn key_shape_and_environment_override_are_warnings() {
    let env = TestEnv::new();
    env.login("https://example.test", "sk-short");

    let output = run(&env, &["doctor"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("warn  credential.key"), "board: {text}");
    assert!(text.contains("sk-…hort"), "board: {text}");
    assert!(!text.contains("sk-short"), "full API key leaked: {text}");

    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    let env_key = "sk-ffffffffffffffffffffffffffffffff";
    let output = env
        .command()
        .arg("doctor")
        .env("TOKENCAMP_API_KEY", env_key)
        .output()
        .expect("run tcamp");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("warn  credential.key"), "board: {text}");
    assert!(text.contains("overrides"), "board: {text}");
    assert!(!text.contains(KEY), "full API key leaked: {text}");
    assert!(!text.contains(env_key), "full API key leaked: {text}");
}

#[test]
fn private_http_base_urls_pass_and_are_normalized() {
    let env = TestEnv::new();
    env.login("http://127.0.0.1:9/v1/", KEY);

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("pass  connection.base_url"), "board: {text}");
    assert!(text.contains("http://127.0.0.1:9"), "board: {text}");
    assert!(
        !text.contains("9/v1"),
        "board must show the normalized URL: {text}"
    );
}

#[test]
fn public_http_and_invalid_base_urls_fail() {
    for raw in ["http://example.com", "not-a-url", "ftp://example.com"] {
        let env = TestEnv::new();
        let output = env
            .command()
            .arg("doctor")
            .env("TOKENCAMP_API_KEY", KEY)
            .env("TOKENCAMP_BASE_URL", raw)
            .output()
            .expect("run tcamp");

        assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
        let text = stdout(&output);
        assert!(
            text.contains("fail  connection.base_url"),
            "{raw} board: {text}"
        );
        assert!(!text.contains(KEY), "full API key leaked: {text}");
    }
}

#[test]
fn doctor_does_not_change_configured_files_or_state() {
    let env = TestEnv::new();
    let server = support::model_server();
    env.login(&server.url(""), KEY);
    for harness in ["claude", "pi", "kimi"] {
        let output = run(&env, &[harness, "--no-launch"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{harness} stderr: {}",
            stderr(&output)
        );
    }
    env.fake_harness("claude", "2.1.316");
    env.fake_harness("pi", "1.4.0");
    env.fake_harness("kimi", "2.1.1");

    let config_before = tree(&env.xdg_config);
    let state_before = tree(&env.xdg_state);
    let requests_before = server.requests().len();

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(tree(&env.xdg_config), config_before);
    assert_eq!(tree(&env.xdg_state), state_before);
    assert_eq!(
        server.requests().len(),
        requests_before,
        "offline doctor must not make network requests"
    );
}

fn tree(path: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    let mut entries = Vec::new();
    collect_tree(path, path, &mut entries);
    entries.sort();
    entries
}

fn collect_tree(root: &Path, path: &Path, entries: &mut Vec<(String, Option<Vec<u8>>)>) {
    let Ok(read_dir) = fs::read_dir(path) else {
        return;
    };
    for entry in read_dir {
        let entry = entry.expect("read entry");
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(root)
            .expect("entry under root")
            .display()
            .to_string();
        let metadata = entry.metadata().expect("entry metadata");
        if metadata.is_dir() {
            entries.push((relative, None));
            collect_tree(root, &entry_path, entries);
        } else {
            entries.push((
                relative,
                Some(fs::read(&entry_path).expect("read fixture file")),
            ));
        }
    }
}

fn configure_all(env: &TestEnv, server: &support::MockServer) {
    env.login(&server.url(""), KEY);
    for harness in ["claude", "pi", "kimi"] {
        let output = run(env, &[harness, "--no-launch"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{harness} stderr: {}",
            stderr(&output)
        );
    }
}

fn write_script(env: &TestEnv, name: &str, body: &str) {
    let path = env.bin_dir.join(name);
    fs::write(&path, body).expect("write fake harness script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("make fake harness executable");
    }
}

#[test]
fn missing_harnesses_warn_but_do_not_fail_the_report() {
    let env = TestEnv::new();
    env.login("https://example.test", KEY);

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert_eq!(text.matches("warn  binary").count(), 3, "board: {text}");
    assert!(text.contains("0 fail"), "board: {text}");
}

#[test]
fn harness_version_floors_and_version_probe_errors_are_reported() {
    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    env.fake_harness("claude", "2.1.304");
    env.fake_harness("pi", "1.4.0");
    env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["doctor"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert_eq!(text.matches("pass  binary").count(), 3, "board: {text}");

    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    env.fake_harness("claude", "2.1.303");
    env.fake_harness("pi", "1.3.9");
    env.fake_harness("kimi", "2.1.0");

    let output = run(&env, &["doctor"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert_eq!(text.matches("warn  binary").count(), 3, "board: {text}");
    assert!(text.contains("below the tested version"), "board: {text}");

    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    write_script(&env, "claude", "#!/bin/sh\nexit 3\n");
    env.fake_harness("pi", "1.4.0");
    env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["doctor"]);
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  binary"), "board: {text}");
    assert!(text.contains("exited with status 3"), "board: {text}");

    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    write_script(&env, "claude", "#!/bin/sh\nprintf 'not a version\\n'\n");
    env.fake_harness("pi", "1.4.0");
    env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["doctor"]);
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("fail  binary"),
        "board: {}",
        stdout(&output)
    );
}

#[test]
fn configured_harnesses_pass_config_and_ownership_checks() {
    let env = TestEnv::new();
    let server = support::model_server();
    configure_all(&env, &server);
    env.fake_harness("claude", "2.1.316");
    env.fake_harness("pi", "1.4.0");
    env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert_eq!(text.matches("pass  config").count(), 3, "board: {text}");
    assert_eq!(text.matches("pass  ownership").count(), 3, "board: {text}");
    assert_eq!(text.matches("skip  backups").count(), 3, "board: {text}");
    assert!(text.contains("warn  messages"), "board: {text}");
}

#[test]
fn drift_is_reported_as_a_warning() {
    let env = TestEnv::new();
    let server = support::model_server();
    configure_all(&env, &server);
    let path = env.claude_overlay_path();
    let contents = fs::read_to_string(&path).expect("read overlay");
    fs::write(
        &path,
        contents.replace("zhipu/glm-5.3", "manual/outside-model"),
    )
    .expect("modify overlay");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("warn  ownership"), "board: {text}");
    assert!(text.contains("modified outside tcamp"), "board: {text}");
    assert!(text.contains("pass  config"), "board: {text}");
}

#[test]
fn corrupt_config_targets_fail() {
    for (harness, contents) in [("claude", "{\n"), ("pi", "{\n"), ("kimi", "[\n")] {
        let env = TestEnv::new();
        env.login("https://example.test", KEY);
        let path = match harness {
            "claude" => env.claude_overlay_path(),
            "pi" => env.pi_models_path(),
            _ => env.kimi_config_path(),
        };
        fs::create_dir_all(path.parent().expect("config parent")).expect("create config parent");
        fs::write(&path, contents).expect("write corrupt config");

        let output = run(&env, &["doctor"]);

        assert_eq!(
            output.status.code(),
            Some(1),
            "{harness} stderr: {}",
            stderr(&output)
        );
        let text = stdout(&output);
        assert!(text.contains("fail  config"), "{harness} board: {text}");
    }
}

#[test]
fn a_target_recorded_by_the_manifest_but_missing_fails() {
    let env = TestEnv::new();
    let server = support::model_server();
    configure_all(&env, &server);
    fs::remove_file(env.claude_overlay_path()).expect("remove overlay");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  config"), "board: {text}");
    assert!(text.contains("file is missing"), "board: {text}");
    assert!(text.contains("fail  ownership"), "board: {text}");
}

#[cfg(unix)]
#[test]
fn an_unwritable_config_target_fails() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    let server = support::model_server();
    env.login(&server.url(""), KEY);
    let output = run(&env, &["pi", "--no-launch"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let path = env.pi_models_path();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).expect("make models read-only");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  config"), "board: {text}");
    assert!(text.contains("not writable"), "board: {text}");
}

#[test]
fn unowned_entries_are_conflicts() {
    for harness in ["claude", "pi", "kimi"] {
        let env = TestEnv::new();
        env.login("https://example.test", KEY);
        let path = match harness {
            "claude" => env.claude_overlay_path(),
            "pi" => env.pi_models_path(),
            _ => env.kimi_config_path(),
        };
        fs::create_dir_all(path.parent().expect("config parent")).expect("create config parent");
        let contents = match harness {
            "claude" => r#"{"env":{"CUSTOM":"1"}}"#,
            "pi" => r#"{"providers":{"tokencamp":{"name":"mine"}}}"#,
            _ => "[providers.tokencamp]\ntype = \"openai\"\n",
        };
        fs::write(&path, contents).expect("write unowned config");

        let output = run(&env, &["doctor"]);

        assert_eq!(
            output.status.code(),
            Some(1),
            "{harness} stderr: {}",
            stderr(&output)
        );
        let text = stdout(&output);
        assert!(text.contains("fail  ownership"), "{harness} board: {text}");
        assert!(
            text.contains("not recorded as managed")
                || text.contains("not managed by tcamp")
                || text.contains("no longer carries"),
            "{harness} board: {text}"
        );
    }
}

#[test]
fn an_unsupported_manifest_version_fails_ownership_without_being_overwritten() {
    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    let manifest = env.xdg_config.join("tokencamp/managed.toml");
    fs::create_dir_all(manifest.parent().expect("manifest directory"))
        .expect("create manifest directory");
    fs::write(&manifest, "version = 2\n").expect("write manifest");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert_eq!(text.matches("fail  ownership").count(), 3, "board: {text}");
    assert_eq!(
        fs::read_to_string(&manifest).expect("read manifest"),
        "version = 2\n",
        "doctor must not overwrite the manifest"
    );
}

#[test]
fn missing_backups_warn() {
    let env = TestEnv::new();
    let server = support::model_server();
    configure_all(&env, &server);

    let overlay = env.claude_overlay_path();
    let contents = fs::read_to_string(&overlay).expect("read overlay");
    fs::write(
        &overlay,
        contents.replace("zhipu/glm-5.3", "manual/outside-model"),
    )
    .expect("modify overlay");
    let output = run(&env, &["claude", "--no-launch"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let backup_dir = env.xdg_state.join("tokencamp/backups/claude");
    let backups: Vec<_> = fs::read_dir(&backup_dir)
        .expect("read backup directory")
        .map(|entry| entry.expect("read backup").path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".bak"))
        })
        .collect();
    assert!(!backups.is_empty(), "configure must create a backup");
    for backup in &backups {
        fs::remove_file(backup).expect("remove backup");
    }

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("warn  ownership"), "board: {text}");
    assert!(text.contains("warn  backups"), "board: {text}");
    assert!(text.contains("missing"), "board: {text}");
}

#[test]
fn claude_shadowing_settings_fail_without_printing_values() {
    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    let settings = env.claude_config.join("settings.json");
    let shadowed = "sk-shadowed0123456789abcdef012345";
    fs::create_dir_all(settings.parent().expect("settings parent")).expect("create settings dir");
    fs::write(
        &settings,
        format!(r#"{{"env":{{"ANTHROPIC_AUTH_TOKEN":"{shadowed}"}}}}"#),
    )
    .expect("write settings");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  shadowing"), "board: {text}");
    assert!(
        text.contains(&settings.display().to_string()),
        "board must name the settings file: {text}"
    );
    assert!(
        text.contains("ANTHROPIC_AUTH_TOKEN"),
        "board must name the key: {text}"
    );
    assert!(!text.contains(shadowed), "shadowed value leaked: {text}");
}

#[cfg(unix)]
#[test]
fn unreadable_claude_managed_settings_warn() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    let managed = env.root.join("managed-settings.json");
    fs::write(&managed, r#"{"env":{}}"#).expect("write managed settings");
    fs::set_permissions(&managed, fs::Permissions::from_mode(0o000))
        .expect("make managed settings unreadable");

    let output = env
        .command()
        .arg("doctor")
        .env("TCAMP_TEST_CLAUDE_MANAGED_SETTINGS", &managed)
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("warn  shadowing"), "board: {text}");
    assert!(
        text.contains(&managed.display().to_string()),
        "board must name the managed path: {text}"
    );
}

#[test]
fn pi_auth_shadowing_and_corruption_fail() {
    for (contents, expected) in [
        (
            r#"{"tokencamp":{"type":"api_key","key":"sk-shadowed"}}"#,
            "tokencamp",
        ),
        ("{\"tokencamp\":", "cannot parse"),
    ] {
        let env = TestEnv::new();
        env.login("https://example.test", KEY);
        fs::write(env.pi_auth_path(), contents).expect("write auth.json");

        let output = run(&env, &["doctor"]);

        assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
        let text = stdout(&output);
        assert!(text.contains("fail  shadowing"), "board: {text}");
        assert!(text.contains(expected), "board: {text}");
        assert!(!text.contains("sk-shadowed"), "auth value leaked: {text}");
    }
}

#[test]
fn an_owned_kimi_literal_api_key_is_a_warning() {
    let env = TestEnv::new();
    let server = support::model_server();
    env.login(&server.url(""), KEY);
    let output = run(&env, &["kimi", "--no-launch"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let literal = "sk-shadowedliteralvalue0123456789";
    let path = env.kimi_config_path();
    let contents = fs::read_to_string(&path).expect("read config");
    fs::write(
        &path,
        contents.replace(
            "type = \"openai\"\n",
            &format!("type = \"openai\"\napi_key = \"{literal}\"\n"),
        ),
    )
    .expect("insert literal api_key");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("warn  shadowing"), "board: {text}");
    assert!(text.contains("literal api_key"), "board: {text}");
    assert!(!text.contains(literal), "literal key leaked: {text}");
}

#[test]
fn an_unowned_kimi_literal_api_key_fails() {
    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    let literal = "sk-shadowedliteralvalue0123456789";
    let path = env.kimi_config_path();
    fs::create_dir_all(path.parent().expect("config parent")).expect("create config parent");
    fs::write(
        &path,
        format!("[providers.tokencamp]\ntype = \"openai\"\napi_key = \"{literal}\"\n"),
    )
    .expect("write config");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  shadowing"), "board: {text}");
    assert!(text.contains("unowned"), "board: {text}");
    assert!(!text.contains(literal), "literal key leaked: {text}");
}

#[test]
fn an_incomplete_or_disabled_static_mapping_fails() {
    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    let output = env
        .command()
        .arg("doctor")
        .env("TCAMP_TEST_DOCTOR_MODEL_GAP", "pi")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  models"), "board: {text}");
    assert!(
        text.contains("static model mapping is invalid"),
        "board: {text}"
    );

    let env = TestEnv::new();
    env.login("https://example.test", KEY);
    let output = env
        .command()
        .arg("doctor")
        .env("TCAMP_TEST_DOCTOR_MODEL_DISABLED", "claude")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("fail  models"),
        "board: {}",
        stdout(&output)
    );
}

#[test]
fn an_owned_claude_overlay_without_an_env_object_fails_config() {
    let env = TestEnv::new();
    let server = support::model_server();
    env.login(&server.url(""), KEY);
    let output = run(&env, &["claude", "--no-launch"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    fs::write(
        env.claude_overlay_path(),
        "{\"_tcamp\":{\"version\":1,\"harness\":\"claude\"},\"env\":\"nope\"}\n",
    )
    .expect("break overlay structure");

    let output = run(&env, &["doctor"]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  config"), "board: {text}");
    assert!(text.contains("no `env` object"), "board: {text}");
}

#[cfg(unix)]
#[test]
fn a_config_target_in_a_read_only_directory_fails() {
    use std::os::unix::fs::PermissionsExt;

    let env = TestEnv::new();
    let server = support::model_server();
    env.login(&server.url(""), KEY);
    let output = run(&env, &["pi", "--no-launch"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let directory = env.pi_agent.clone();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o555))
        .expect("make parent directory read-only");
    let output = run(&env, &["doctor"]);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))
        .expect("restore parent directory");

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("fail  config"), "board: {text}");
    assert!(text.contains("not writable"), "board: {text}");
}

#[cfg(unix)]
#[test]
fn doctor_uses_color_on_a_tty_unless_disabled() {
    let env = TestEnv::new();
    env.login("https://example.test", KEY);

    let mut command = env.std_command();
    command.arg("doctor").env("TERM", "xterm-256color");
    let output = support::pty::run_pty(command, &[]);
    assert!(
        output.output.contains('\u{1b}'),
        "TTY output must be styled: {:?}",
        output.output
    );

    let mut command = env.std_command();
    command
        .args(["--no-color", "doctor"])
        .env("TERM", "xterm-256color");
    let output = support::pty::run_pty(command, &[]);
    assert!(
        !output.output.contains('\u{1b}'),
        "--no-color must suppress styles: {:?}",
        output.output
    );
}
