mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use support::{MockResponse, MockServer, TestEnv, model_server};
use toml_edit::{DocumentMut, Item, Table};

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

fn config_path(env: &TestEnv) -> PathBuf {
    env.kimi_config_path()
}

fn config_text(env: &TestEnv) -> String {
    fs::read_to_string(config_path(env)).expect("read config.toml")
}

fn config_document(env: &TestEnv) -> DocumentMut {
    config_text(env).parse().expect("config.toml is valid TOML")
}

fn provider(document: &DocumentMut) -> &Table {
    document["providers"]
        .as_table()
        .expect("providers table")
        .get("tokencamp")
        .and_then(Item::as_table)
        .expect("providers.tokencamp table")
}

fn alias<'a>(document: &'a DocumentMut, name: &str) -> &'a Table {
    document["models"]
        .as_table()
        .expect("models table")
        .get(name)
        .and_then(Item::as_table)
        .unwrap_or_else(|| panic!("missing model alias {name}"))
}

fn backup_files(env: &TestEnv) -> Vec<PathBuf> {
    let directory = env.xdg_state.join("tokencamp/backups/kimi");
    let mut paths: Vec<_> = match fs::read_dir(&directory) {
        Ok(entries) => entries
            .map(|entry| entry.expect("read backup entry").path())
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("read backup directory: {error}"),
    };
    paths.sort();
    paths
}

fn header_positions(text: &str, headers: &[&str]) -> Vec<usize> {
    headers
        .iter()
        .map(|header| {
            text.find(header)
                .unwrap_or_else(|| panic!("missing {header:?} in:\n{text}"))
        })
        .collect()
}

fn manifest_document(env: &TestEnv) -> DocumentMut {
    fs::read_to_string(env.xdg_config.join("tokencamp/managed.toml"))
        .expect("read manifest")
        .parse()
        .expect("parse manifest")
}

fn manifest_entry<'a>(manifest: &'a DocumentMut, path: &Path) -> &'a Table {
    manifest["targets"]
        .as_table()
        .expect("targets table")
        .get(&path.display().to_string())
        .and_then(Item::as_table)
        .unwrap_or_else(|| panic!("missing manifest entry for {}", path.display()))
}

#[test]
fn configure_writes_config_toml_with_the_provider_and_five_aliases() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains(&format!("Configured kimi: {}", config_path(&env).display())),
        "stdout: {}",
        stdout(&output)
    );

    let document = config_document(&env);
    assert_eq!(
        document["default_model"].as_str(),
        Some("tokencamp/glm-5.3")
    );

    let provider = provider(&document);
    assert_eq!(
        provider.get("type").and_then(Item::as_str),
        Some("openai"),
        "type = anthropic would send x-api-key"
    );
    assert_eq!(
        provider.get("base_url").and_then(Item::as_str),
        Some(format!("{}/v1", server.url("")).as_str())
    );
    assert_eq!(
        provider.get("api_key_env").and_then(Item::as_str),
        Some("TOKENCAMP_API_KEY")
    );
    assert!(
        provider.get("api_key").is_none(),
        "the provider entry must not hold a literal key"
    );

    let aliases = document["models"]
        .as_table()
        .expect("models table")
        .iter()
        .map(|(name, _)| name.to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        aliases,
        vec![
            "tokencamp/glm-5.3",
            "tokencamp/deepseek-v4-pro",
            "tokencamp/glm-5.3-flash",
            "tokencamp/kimi-k2.7-code",
            "tokencamp/auto",
        ]
    );

    for (name, model, context) in [
        ("tokencamp/glm-5.3", "zhipu/glm-5.3", 1_048_576),
        (
            "tokencamp/deepseek-v4-pro",
            "deepseek/deepseek-v4-pro",
            1_048_576,
        ),
        ("tokencamp/glm-5.3-flash", "zhipu/glm-5.3-flash", 1_048_576),
        ("tokencamp/kimi-k2.7-code", "kimi/kimi-k2.7-code", 262_144),
        ("tokencamp/auto", "tokencamp/auto", 200_000),
    ] {
        let entry = alias(&document, name);
        assert_eq!(
            entry.get("provider").and_then(Item::as_str),
            Some("tokencamp"),
            "provider for {name}"
        );
        assert_eq!(
            entry.get("model").and_then(Item::as_str),
            Some(model),
            "canonical model for {name}"
        );
        assert_eq!(
            entry.get("max_context_size").and_then(Item::as_integer),
            Some(context),
            "context for {name}"
        );
        assert_eq!(
            entry.get("max_output_size").and_then(Item::as_integer),
            Some(8192),
            "max output for {name}"
        );
        assert!(
            entry
                .get("display_name")
                .and_then(Item::as_str)
                .is_some_and(|name| !name.is_empty()),
            "display_name for {name}"
        );
    }

    assert!(!config_text(&env).contains(KEY));
}

#[test]
fn configure_preserves_toml_comments_blank_lines_table_order_and_unknown_fields() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let existing = r#"# top comment
theme = "dark"

# general comment
[general]
name = "keep"

# provider comment
[providers.tokencamp]
type = "anthropic" # old type
base_url = "https://old.example/v1"
api_key_env = "OLD"
custom = "keep-me"

# other provider
[providers.other]
type = "openai"

# user model
[models."user/model"]
provider = "other"

# managed alias comment
[models."tokencamp/glm-5.3"]
provider = "tokencamp"
model = "old/model" # old model
max_context_size = 1
max_output_size = 1
custom = "keep-me"
display_name = "user name" # keep comment
"#;
    fs::write(config_path(&env), existing).expect("write config.toml");

    let output = run(&env, &["kimi", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = config_text(&env);
    for marker in [
        "# top comment",
        "# general comment",
        "# provider comment",
        "# other provider",
        "# user model",
        "# managed alias comment",
        "# old type",
        "# old model",
        "# keep comment",
        "custom = \"keep-me\"",
        "[providers.other]",
        "name = \"keep\"",
        "[models.\"user/model\"]",
    ] {
        assert!(text.contains(marker), "missing {marker:?} in:\n{text}");
    }
    assert!(
        !text.contains("tcamp-managed"),
        "tcamp must not add marker comments:\n{text}"
    );

    let positions = header_positions(
        &text,
        &[
            "[general]",
            "[providers.tokencamp]",
            "[providers.other]",
            "[models.\"user/model\"]",
            "[models.\"tokencamp/glm-5.3\"]",
            "[models.\"tokencamp/deepseek-v4-pro\"]",
        ],
    );
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "existing tables must stay in order and new tables must be appended: {positions:?}\n{text}"
    );

    let document = config_document(&env);
    assert_eq!(document["theme"].as_str(), Some("dark"));
    assert_eq!(
        document["default_model"].as_str(),
        Some("tokencamp/glm-5.3")
    );

    let provider = provider(&document);
    assert_eq!(provider.get("type").and_then(Item::as_str), Some("openai"));
    assert_eq!(
        provider.get("base_url").and_then(Item::as_str),
        Some(format!("{}/v1", server.url("")).as_str())
    );
    assert_eq!(
        provider.get("api_key_env").and_then(Item::as_str),
        Some("TOKENCAMP_API_KEY")
    );
    assert_eq!(
        provider.get("custom").and_then(Item::as_str),
        Some("keep-me")
    );

    let managed = alias(&document, "tokencamp/glm-5.3");
    assert_eq!(
        managed.get("model").and_then(Item::as_str),
        Some("zhipu/glm-5.3")
    );
    assert_eq!(
        managed.get("max_context_size").and_then(Item::as_integer),
        Some(1_048_576)
    );
    assert_eq!(
        managed.get("max_output_size").and_then(Item::as_integer),
        Some(8192)
    );
    assert_eq!(
        managed.get("display_name").and_then(Item::as_str),
        Some("GLM 5.3")
    );
    assert_eq!(
        managed.get("custom").and_then(Item::as_str),
        Some("keep-me")
    );

    let other = document["models"]
        .as_table()
        .expect("models")
        .get("user/model")
        .and_then(Item::as_table)
        .expect("user model");
    assert_eq!(other.get("provider").and_then(Item::as_str), Some("other"));
}

#[test]
fn corrupt_toml_is_rejected_with_a_location_and_never_rewritten() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let corrupt = "default_model = \"x\"\n[providers.tokencamp\ntype = \"openai\"\n";
    fs::write(config_path(&env), corrupt).expect("write corrupt config.toml");

    for args in [
        vec!["kimi", "--no-launch"],
        vec!["kimi", "--no-launch", "--force"],
        vec!["kimi", "--no-launch", "--dry-run"],
    ] {
        let output = run(&env, &args);
        assert_eq!(output.status.code(), Some(2), "args: {args:?}");
        let message = stderr(&output);
        assert!(
            message.contains("config_unreadable"),
            "args: {args:?}: {message}"
        );
        assert!(
            message.contains(&config_path(&env).display().to_string()),
            "args: {args:?}: {message}"
        );
        assert!(message.contains("line "), "args: {args:?}: {message}");
        assert!(message.contains("column "), "args: {args:?}: {message}");
        assert_eq!(
            fs::read_to_string(config_path(&env)).expect("read config"),
            corrupt
        );
    }
    assert!(backup_files(&env).is_empty());
}

#[test]
fn structurally_invalid_entries_are_rejected_before_writing() {
    for invalid in [
        "providers = \"nope\"\n",
        "[providers]\ntokencamp = \"nope\"\n",
        "models = \"nope\"\n",
        "[models]\n\"tokencamp/auto\" = \"nope\"\n",
        "default_model = 1\n",
    ] {
        let env = TestEnv::new();
        let server = model_server();
        env.login(&server.url(""), KEY);
        fs::write(config_path(&env), invalid).expect("write config.toml");

        let output = run(&env, &["kimi", "--no-launch", "--force"]);

        assert_eq!(
            output.status.code(),
            Some(2),
            "config {invalid:?}: stderr: {}",
            stderr(&output)
        );
        let message = stderr(&output);
        assert!(
            message.contains("config_unreadable"),
            "config {invalid:?}: {message}"
        );
        assert!(
            message.contains(&config_path(&env).display().to_string()),
            "config {invalid:?}: {message}"
        );
        assert_eq!(
            fs::read_to_string(config_path(&env)).expect("read config"),
            invalid
        );
        assert!(backup_files(&env).is_empty());
    }
}

#[test]
fn an_unowned_provider_entry_is_a_conflict_until_forced() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let existing = r#"# user provider
[providers.tokencamp]
name = "mine"
type = "openai"
base_url = "https://user.example/v1"
unknown = "keep-me"
"#;
    fs::write(config_path(&env), existing).expect("write config.toml");

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_conflict"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(config_path(&env)).expect("read config"),
        existing
    );
    assert!(backup_files(&env).is_empty());

    let output = run(&env, &["kimi", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let backups = backup_files(&env);
    assert_eq!(backups.len(), 1, "backups: {backups:?}");
    assert_eq!(
        fs::read_to_string(&backups[0]).expect("read backup"),
        existing
    );
    let document = config_document(&env);
    let provider = provider(&document);
    assert_eq!(
        provider.get("unknown").and_then(Item::as_str),
        Some("keep-me")
    );
    assert_eq!(
        provider.get("base_url").and_then(Item::as_str),
        Some(format!("{}/v1", server.url("")).as_str())
    );

    let manifest = manifest_document(&env);
    let entry = manifest_entry(&manifest, &config_path(&env));
    assert_eq!(entry["adopted"].as_bool(), Some(true));
}

#[test]
fn an_unowned_alias_without_a_provider_is_a_conflict_until_forced() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let existing = r#"# user alias
[models."tokencamp/auto"]
provider = "tokencamp"
model = "user/auto"
custom = "keep-me"
"#;
    fs::write(config_path(&env), existing).expect("write config.toml");

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_conflict"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["kimi", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let document = config_document(&env);
    let adopted = alias(&document, "tokencamp/auto");
    assert_eq!(
        adopted.get("custom").and_then(Item::as_str),
        Some("keep-me")
    );
    assert_eq!(
        adopted.get("model").and_then(Item::as_str),
        Some("tokencamp/auto")
    );

    let manifest = manifest_document(&env);
    let entry = manifest_entry(&manifest, &config_path(&env));
    let ids: Vec<&str> = entry["model_ids"]
        .as_array()
        .expect("model ids")
        .iter()
        .map(|value| value.as_str().expect("model id"))
        .collect();
    assert_eq!(
        ids,
        vec![
            "tokencamp/glm-5.3",
            "tokencamp/deepseek-v4-pro",
            "tokencamp/glm-5.3-flash",
            "tokencamp/kimi-k2.7-code",
        ],
        "the pre-existing alias must not be claimed as tcamp-created"
    );
}

#[test]
fn configure_records_ownership_and_reports_drift() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);

    let output = run(&env, &["kimi", "--no-launch"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let manifest = manifest_document(&env);
    let entry = manifest_entry(&manifest, &config_path(&env));
    assert_eq!(entry["harness"].as_str(), Some("kimi"));
    assert_eq!(entry["provider_entry"].as_str(), Some("tokencamp"));
    assert_eq!(entry["created_file"].as_bool(), Some(true));
    let ids: Vec<&str> = entry["model_ids"]
        .as_array()
        .expect("model ids")
        .iter()
        .map(|value| value.as_str().expect("model id"))
        .collect();
    assert_eq!(
        ids,
        vec![
            "tokencamp/glm-5.3",
            "tokencamp/deepseek-v4-pro",
            "tokencamp/glm-5.3-flash",
            "tokencamp/kimi-k2.7-code",
            "tokencamp/auto",
        ]
    );
    assert_eq!(
        entry["defaults"]["default_model"].as_str(),
        Some("tokencamp/glm-5.3")
    );
    assert!(
        !entry["last_written_hash"]
            .as_str()
            .expect("hash")
            .is_empty()
    );

    let edited = config_text(&env).replace(
        &format!("{}/v1", server.url("")),
        "https://drifted.example/v1",
    );
    assert_ne!(edited, config_text(&env));
    fs::write(config_path(&env), &edited).expect("edit config.toml");

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_drift"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        provider(&config_document(&env))
            .get("base_url")
            .and_then(Item::as_str),
        Some(format!("{}/v1", server.url("")).as_str())
    );
}

#[test]
fn a_missing_manifest_cannot_take_ownership_of_an_existing_entry() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    run(&env, &["kimi", "--no-launch"]);
    fs::remove_file(env.xdg_config.join("tokencamp/managed.toml")).expect("delete manifest");

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_conflict"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn existing_default_model_is_preserved_and_an_empty_one_is_filled() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    fs::write(
        config_path(&env),
        "default_model = \"user/model\" # keep my choice\n",
    )
    .expect("write config.toml");

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = config_text(&env);
    assert!(text.contains("default_model = \"user/model\" # keep my choice"));
    let manifest = manifest_document(&env);
    let entry = manifest_entry(&manifest, &config_path(&env));
    assert!(
        entry.get("defaults").is_none(),
        "a preserved default must not be recorded as written"
    );

    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    fs::write(config_path(&env), "default_model = \"\"\n").expect("write config.toml");

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        config_document(&env)["default_model"].as_str(),
        Some("tokencamp/glm-5.3")
    );
    let manifest = manifest_document(&env);
    let entry = manifest_entry(&manifest, &config_path(&env));
    assert_eq!(
        entry["defaults"]["default_model"].as_str(),
        Some("tokencamp/glm-5.3")
    );
}

#[test]
fn configure_falls_back_to_home_kimi_code_dir_when_the_override_is_unset() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);

    let output = env
        .command()
        .args(["kimi", "--no-launch"])
        .env_remove("KIMI_CODE_HOME")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(env.home.join(".kimi-code/config.toml").exists());
}

#[test]
fn configure_does_not_write_project_or_unrelated_files() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let project = env.root.join("project");
    fs::create_dir_all(&project).expect("create project directory");

    let output = env
        .command()
        .current_dir(&project)
        .args(["kimi", "--no-launch"])
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        fs::read_dir(&project).expect("read project").count(),
        0,
        "project-level configuration must not be written"
    );
}

#[test]
fn an_owned_literal_api_key_is_backed_up_and_removed() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    run(&env, &["kimi", "--no-launch"]);

    let mut document = config_document(&env);
    document["providers"]["tokencamp"]["api_key"] = Item::Value(KEY.into());
    fs::write(config_path(&env), document.to_string()).expect("add a literal api_key");
    let with_key = config_text(&env);
    assert!(with_key.contains(KEY));

    let output = run(&env, &["kimi", "--no-launch", "--dry-run"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !combined.contains(KEY),
        "the diff leaked the literal key: {combined}"
    );
    assert!(combined.contains("sk-…cdef"), "diff: {combined}");
    assert_eq!(
        config_text(&env),
        with_key,
        "a dry-run must not change the file"
    );

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !combined.contains(KEY),
        "the output leaked the literal key: {combined}"
    );
    assert!(
        stderr(&output).contains("config_drift"),
        "stderr: {}",
        stderr(&output)
    );
    let document = config_document(&env);
    let provider = provider(&document);
    assert!(
        provider.get("api_key").is_none(),
        "the owned literal key must be removed"
    );
    assert_eq!(
        provider.get("api_key_env").and_then(Item::as_str),
        Some("TOKENCAMP_API_KEY")
    );

    let backups = backup_files(&env);
    assert_eq!(backups.len(), 1, "backups: {backups:?}");
    assert!(
        fs::read_to_string(&backups[0])
            .expect("read backup")
            .contains(KEY),
        "the backup must preserve the original file for recovery"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&backups[0])
            .expect("backup metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "backups must stay private");
    }
}

#[test]
fn a_parse_error_never_echoes_a_literal_api_key() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let corrupt = format!("[providers.tokencamp]\napi_key = \"{KEY}\n");
    fs::write(config_path(&env), corrupt).expect("write corrupt config.toml");

    let output = run(&env, &["kimi", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !combined.contains(KEY),
        "the parse error leaked the literal key: {combined}"
    );
    assert!(
        stderr(&output).contains("config_unreadable"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn an_unowned_literal_api_key_is_a_conflict_until_forced() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let existing = format!(
        "[providers.tokencamp]\ntype = \"openai\"\napi_key = \"{KEY}\"\napi_key_env = \"OLD\"\n"
    );
    fs::write(config_path(&env), &existing).expect("write config.toml");

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !combined.contains(KEY),
        "the conflict message leaked the literal key: {combined}"
    );
    assert!(
        stderr(&output).contains("config_conflict"),
        "stderr: {}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(config_path(&env)).expect("read config"),
        existing
    );
    assert!(backup_files(&env).is_empty());

    let output = run(&env, &["kimi", "--no-launch", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !combined.contains(KEY),
        "the forced run leaked the literal key: {combined}"
    );
    let document = config_document(&env);
    let provider = provider(&document);
    assert!(provider.get("api_key").is_none());
    assert_eq!(
        provider.get("api_key_env").and_then(Item::as_str),
        Some("TOKENCAMP_API_KEY")
    );
    let backups = backup_files(&env);
    assert_eq!(backups.len(), 1, "backups: {backups:?}");
    assert!(
        fs::read_to_string(&backups[0])
            .expect("read backup")
            .contains(KEY)
    );
}

fn all_model_server() -> MockServer {
    let mut routes = support::model_routes();
    routes.push((
        "/v1/models/tokencamp/auto",
        MockResponse::json(200, r#"{"id":"model"}"#),
    ));
    MockServer::start_routes(routes)
}

#[test]
fn launch_execs_kimi_with_the_alias_and_injected_key() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let kimi = env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["kimi"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        kimi.args(),
        vec!["-m".to_string(), "tokencamp/glm-5.3".to_string()]
    );
    assert_eq!(kimi.env_value("TOKENCAMP_API_KEY").as_deref(), Some(KEY));
    assert!(
        stderr(&output).contains("Launching kimi ..."),
        "stderr: {}",
        stderr(&output)
    );
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(!combined.contains(KEY), "output leaked the key: {combined}");
    assert!(!config_text(&env).contains(KEY));
    assert!(!env.kimi_home.join("auth.json").exists());
}

#[test]
fn arguments_after_double_dash_are_passed_through() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let kimi = env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["kimi", "--", "--nope", "hello"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        kimi.args(),
        vec![
            "-m".to_string(),
            "tokencamp/glm-5.3".to_string(),
            "--nope".to_string(),
            "hello".to_string(),
        ]
    );
}

#[test]
fn no_launch_writes_the_config_even_when_kimi_is_missing() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(config_path(&env).exists());
}

#[test]
fn launch_requires_the_binary_without_no_launch() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);

    let output = run(&env, &["kimi"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("harness_not_found"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        server.requests().is_empty(),
        "the binary check must run before the model checks"
    );
    assert!(!config_path(&env).exists());
}

#[test]
fn model_aliases_and_canonical_ids_resolve_to_the_alias() {
    for (input, alias) in [
        ("tokencamp/glm-5.3", "tokencamp/glm-5.3"),
        ("tokencamp/deepseek-v4-pro", "tokencamp/deepseek-v4-pro"),
        ("tokencamp/glm-5.3-flash", "tokencamp/glm-5.3-flash"),
        ("tokencamp/kimi-k2.7-code", "tokencamp/kimi-k2.7-code"),
        ("tokencamp/auto", "tokencamp/auto"),
        ("zhipu/glm-5.3", "tokencamp/glm-5.3"),
        ("deepseek/deepseek-v4-pro", "tokencamp/deepseek-v4-pro"),
        ("zhipu/glm-5.3-flash", "tokencamp/glm-5.3-flash"),
        ("kimi/kimi-k2.7-code", "tokencamp/kimi-k2.7-code"),
    ] {
        let env = TestEnv::new();
        let server = all_model_server();
        env.login(&server.url(""), KEY);
        let kimi = env.fake_harness("kimi", "2.1.1");

        let output = run(&env, &["kimi", "--model", input]);

        assert_eq!(
            output.status.code(),
            Some(0),
            "model {input}: stderr: {}",
            stderr(&output)
        );
        assert_eq!(
            kimi.args(),
            vec!["-m".to_string(), alias.to_string()],
            "model {input}"
        );
    }
}

#[test]
fn an_invalid_model_exits_2_and_lists_the_five_aliases() {
    let env = TestEnv::new();
    let server = all_model_server();
    env.login(&server.url(""), KEY);
    let kimi = env.fake_harness("kimi", "2.1.1");

    for invalid in [
        "",
        "not-a-model",
        "glm-5.3",
        "auto",
        "@preset/fast",
        "~family-latest",
    ] {
        let output = run(&env, &["kimi", "--model", invalid]);

        assert_eq!(
            output.status.code(),
            Some(2),
            "model {invalid:?}: stderr: {}",
            stderr(&output)
        );
        let message = stderr(&output);
        assert!(message.contains("[usage]"), "model {invalid:?}: {message}");
        for available in [
            "tokencamp/glm-5.3",
            "tokencamp/deepseek-v4-pro",
            "tokencamp/glm-5.3-flash",
            "tokencamp/kimi-k2.7-code",
            "tokencamp/auto",
        ] {
            assert!(
                message.contains(available),
                "model {invalid:?} must list {available}: {message}"
            );
        }
    }

    assert!(!kimi.ran());
    assert!(!config_path(&env).exists());
    assert!(server.requests().is_empty());
}

#[test]
fn tokencamp_model_overrides_the_launch_and_an_empty_value_is_ignored() {
    let env = TestEnv::new();
    let server = all_model_server();
    env.login(&server.url(""), KEY);
    let kimi = env.fake_harness("kimi", "2.1.1");

    let output = env
        .command()
        .args(["kimi"])
        .env("TOKENCAMP_MODEL", "zhipu/glm-5.3-flash")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(kimi.args()[1], "tokencamp/glm-5.3-flash");

    let env = TestEnv::new();
    let server = all_model_server();
    env.login(&server.url(""), KEY);
    let kimi = env.fake_harness("kimi", "2.1.1");
    let output = env
        .command()
        .args(["kimi"])
        .env("TOKENCAMP_MODEL", "")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(kimi.args()[1], "tokencamp/glm-5.3");
}

#[test]
fn the_model_flag_wins_over_the_environment() {
    let env = TestEnv::new();
    let server = all_model_server();
    env.login(&server.url(""), KEY);
    let kimi = env.fake_harness("kimi", "2.1.1");

    let output = env
        .command()
        .args(["kimi", "--model", "tokencamp/auto"])
        .env("TOKENCAMP_MODEL", "kimi/kimi-k2.7-code")
        .output()
        .expect("run tcamp");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(kimi.args()[1], "tokencamp/auto");
}

#[test]
fn the_model_override_does_not_change_the_config() {
    let env = TestEnv::new();
    let server = all_model_server();
    env.login(&server.url(""), KEY);
    env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["kimi", "--model", "tokencamp/auto"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let document = config_document(&env);
    assert_eq!(
        document["default_model"].as_str(),
        Some("tokencamp/glm-5.3")
    );
    assert_eq!(
        document["models"]
            .as_table()
            .expect("models")
            .iter()
            .count(),
        5
    );
    assert_eq!(
        alias(&document, "tokencamp/glm-5.3")
            .get("max_context_size")
            .and_then(Item::as_integer),
        Some(1_048_576)
    );
}

#[test]
fn a_version_below_the_floor_warns_but_still_launches() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    let kimi = env.fake_harness("kimi", "2.1.0");

    let output = run(&env, &["kimi"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(kimi.ran());
    let message = stderr(&output);
    assert!(
        message.contains("below the tested version"),
        "stderr: {message}"
    );
    assert!(message.contains("2.1.1"), "stderr: {message}");
}

#[test]
fn a_version_at_the_floor_does_not_warn() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["kimi"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        !stderr(&output).contains("below the tested version"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn preflight_checks_only_the_launch_model() {
    let env = TestEnv::new();
    let server = all_model_server();
    env.login(&server.url(""), KEY);
    env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["kimi"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let paths: Vec<String> = server
        .requests()
        .into_iter()
        .map(|request| request.path)
        .collect();
    assert_eq!(paths, vec!["/v1/models/zhipu/glm-5.3".to_string()]);

    let env = TestEnv::new();
    let server = all_model_server();
    env.login(&server.url(""), KEY);
    env.fake_harness("kimi", "2.1.1");

    let output = run(&env, &["kimi", "--model", "tokencamp/auto"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let paths: Vec<String> = server
        .requests()
        .into_iter()
        .map(|request| request.path)
        .collect();
    assert_eq!(paths, vec!["/v1/models/tokencamp/auto".to_string()]);
}

#[test]
fn a_missing_model_exits_2_without_writing() {
    let env = TestEnv::new();
    let server = MockServer::start_routes(vec![(
        "/v1/models/zhipu/glm-5.3",
        MockResponse::json(404, r#"{"error":{"code":"model_not_found"}}"#),
    )]);
    env.login(&server.url(""), KEY);

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("zhipu/glm-5.3"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!config_path(&env).exists());
}

#[test]
fn a_model_check_network_failure_warns_and_continues() {
    let env = TestEnv::new();
    let base_url = env.unused_loopback_url();
    env.login(&base_url, KEY);

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("model_check_failed"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(config_path(&env).exists());
}

#[test]
fn dry_run_does_not_contact_the_models_endpoint() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);

    let output = run(&env, &["kimi", "--dry-run"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(server.requests().is_empty(), "dry-run must stay offline");
    assert!(!config_path(&env).exists());
}

#[test]
fn dry_run_reports_an_unowned_conflict_unless_forced() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    fs::write(
        config_path(&env),
        "[providers.tokencamp]\nname = \"mine\"\n",
    )
    .expect("write config.toml");

    let output = run(&env, &["kimi", "--no-launch", "--dry-run"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("config_conflict"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("--force"),
        "stderr: {}",
        stderr(&output)
    );

    let output = run(&env, &["kimi", "--no-launch", "--dry-run", "--force"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("+++ "),
        "stdout: {}",
        stdout(&output)
    );
    assert!(backup_files(&env).is_empty());
}

#[test]
fn configure_json_reports_create_then_none() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);

    let output = run(&env, &["kimi", "--no-launch", "--json"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("one JSON object");
    assert_eq!(value["ok"], true);
    assert_eq!(value["key"]["masked"], "sk-…cdef");
    assert_eq!(
        value["harnesses"]["kimi"]["target"],
        config_path(&env).display().to_string()
    );
    assert_eq!(value["harnesses"]["kimi"]["action"], "create");

    let output = run(&env, &["kimi", "--no-launch", "--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("one JSON object");
    assert_eq!(value["harnesses"]["kimi"]["action"], "none");
    assert!(!stdout(&output).contains(KEY));
}

#[test]
fn dry_run_json_lists_the_config_change() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);

    let output = run(&env, &["kimi", "--no-launch", "--dry-run", "--json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(output.stderr.is_empty(), "stderr: {}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("one JSON object");
    assert_eq!(value["ok"], true);
    let changes = value["changes"].as_array().expect("changes array");
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["path"], config_path(&env).display().to_string());
    assert_eq!(changes[0]["action"], "create");
    let diff = changes[0]["diff"].as_str().expect("diff");
    assert!(diff.contains("\"openai\""), "diff: {diff}");
    assert!(diff.contains("max_output_size = 8192"), "diff: {diff}");
    assert!(
        diff.contains("api_key_env = \"TOKENCAMP_API_KEY\""),
        "the env reference must not be redacted as a secret: {diff}"
    );
    assert!(!stdout(&output).contains(KEY));
}

#[test]
fn a_second_configure_is_quiet_and_writes_no_backup() {
    let env = TestEnv::new();
    let server = model_server();
    env.login(&server.url(""), KEY);
    run(&env, &["kimi", "--no-launch"]);

    let output = run(&env, &["kimi", "--no-launch"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("No changes."),
        "stdout: {}",
        stdout(&output)
    );
    assert!(backup_files(&env).is_empty());
}
