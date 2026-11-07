#![cfg(unix)]
mod support;

use std::fs;

use support::dist;

const README_EN: &str = "README.md";
const README_ZH: &str = "README.zh-CN.md";

fn read(relative: &str) -> String {
    fs::read_to_string(dist::repo_root().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected document to contain {needle:?}"
    );
}

fn assert_contains_ignoring_case(haystack: &str, needle: &str) {
    assert!(
        haystack.to_lowercase().contains(&needle.to_lowercase()),
        "expected document to contain {needle:?} (case-insensitive)"
    );
}

fn h2_headings(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with("## "))
        .collect()
}

#[test]
fn readmes_are_linked_and_aligned_in_structure() {
    let english = read(README_EN);
    let chinese = read(README_ZH);
    assert_contains(&english, "README.zh-CN.md");
    assert_contains(&chinese, "README.md");

    let english_headings = h2_headings(&english);
    let chinese_headings = h2_headings(&chinese);
    assert_eq!(
        english_headings.len(),
        chinese_headings.len(),
        "README.md and README.zh-CN.md must cover the same sections\nEN: {english_headings:?}\nZH: {chinese_headings:?}"
    );

    let sections = [
        ("## Install", "## 安装"),
        ("## Quickstart", "## 快速开始"),
        ("## Configuration", "## 配置"),
        ("## Troubleshooting", "## 故障排查"),
        ("## Security", "## 安全"),
        ("## Known limitations", "## 已知限制"),
        ("## Uninstall", "## 卸载"),
        ("## License", "## 许可证"),
    ];
    for (english_heading, chinese_heading) in sections {
        assert_contains(&english, english_heading);
        assert_contains(&chinese, chinese_heading);
    }
}

#[test]
fn readmes_cover_installation_checksums_platforms_and_gatekeeper() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    let shared = [
        "TCAMP_VERSION",
        "--version",
        "TCAMP_INSTALL_DIR",
        "SHA256SUMS",
        "xattr",
        "PATH",
        "Gatekeeper",
        "install.sh",
    ];
    for token in shared {
        assert_contains(&english, token);
        assert_contains(&chinese, token);
    }

    assert_contains(
        &english,
        "https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh",
    );
    assert_contains(
        &chinese,
        "https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh",
    );
    assert_contains(&english, "sh -s -- --version");
    assert_contains(&chinese, "sh -s -- --version");
    assert_contains_ignoring_case(&english, "checksum");
    assert_contains(&chinese, "校验和");
    assert_contains(&english, "macOS");
    assert_contains(&chinese, "macOS");
    assert_contains(&english, "Linux");
    assert_contains(&chinese, "Linux");
    assert_contains(&english, "arm64");
    assert_contains(&chinese, "arm64");
    assert_contains(&english, "x86_64");
    assert_contains(&chinese, "x86_64");
    assert_contains_ignoring_case(&english, "right-click");
    assert_contains(&chinese, "右键");
    assert_contains(&english, "xattr -d com.apple.quarantine");
    assert_contains(&chinese, "xattr -d com.apple.quarantine");
    assert_contains_ignoring_case(&english, "does not remove quarantine");
    assert_contains(&chinese, "不会自动移除 quarantine");
}

#[test]
fn readmes_cover_quickstart_first_run_and_non_interactive_use() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    for command in ["tcamp login", "tcamp claude", "tcamp pi", "tcamp kimi"] {
        assert_contains(&english, command);
        assert_contains(&chinese, command);
    }
    for token in ["--no-launch", "--key-stdin", "TOKENCAMP_API_KEY"] {
        assert_contains(&english, token);
        assert_contains(&chinese, token);
    }
    assert_contains_ignoring_case(&english, "first run");
    assert_contains(&chinese, "首次");
    assert_contains_ignoring_case(&english, "non-TTY");
    assert_contains(&chinese, "非 TTY");
    assert_contains_ignoring_case(&english, "Write it?");
    assert_contains(&chinese, "Write it?");
    assert_contains_ignoring_case(&english, "CLI output");
    assert_contains_ignoring_case(&english, "English");
    assert_contains(&chinese, "CLI 输出");
    assert_contains(&chinese, "英文");
}

#[test]
fn readmes_cover_config_targets_overlays_ownership_and_secrets() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    for token in [
        "Harness",
        "Config target",
        "Overlay",
        "Ownership manifest",
        "Managed entry",
        "Model mapping",
        "Connection",
        "Credential",
        "Active connection",
        "Drift",
        "Conflict",
    ] {
        assert_contains(&english, token);
        assert_contains(&chinese, token);
    }

    for token in ["--settings", "models.json", "config.toml", "managed.toml"] {
        assert_contains(&english, token);
        assert_contains(&chinese, token);
    }
    for token in ["--force", "--yes", "--dry-run"] {
        assert_contains(&english, token);
        assert_contains(&chinese, token);
    }
    assert_contains(&english, "harnesses/claude/settings.json");
    assert_contains(&chinese, "harnesses/claude/settings.json");
    assert_contains(&english, ".pi/agent/models.json");
    assert_contains(&chinese, ".pi/agent/models.json");
    assert_contains(&english, ".kimi-code/config.toml");
    assert_contains(&chinese, ".kimi-code/config.toml");
    assert_contains(&english, "providers.tokencamp");
    assert_contains(&chinese, "providers.tokencamp");
    assert_contains(&english, "api_key_env");
    assert_contains(&chinese, "api_key_env");
    assert_contains_ignoring_case(&english, "no API key");
    assert_contains(&chinese, "不写入");
    assert_contains(&chinese, "API key");
}

#[test]
fn readmes_cover_troubleshooting_paths() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    for token in ["401", "402", "doctor --network"] {
        assert_contains(&english, token);
        assert_contains(&chinese, token);
    }
    assert_contains_ignoring_case(&english, "invalid or expired");
    assert_contains(&chinese, "无效");
    assert_contains_ignoring_case(&english, "billing");
    assert_contains(&chinese, "账单");
    assert_contains_ignoring_case(&english, "not installed");
    assert_contains(&chinese, "未安装");
    assert_contains_ignoring_case(&english, "corrupt");
    assert_contains(&chinese, "损坏");
    assert_contains(&english, "anthropic deployment");
    assert_contains(&chinese, "anthropic deployment");
    assert_contains(&english, "provider_type: anthropic");
    assert_contains(&chinese, "provider_type: anthropic");
    for (english_version, chinese_version) in [
        ("2.1.304", "2.1.304"),
        ("1.4.0", "1.4.0"),
        ("2.1.1", "2.1.1"),
    ] {
        assert_contains(&english, english_version);
        assert_contains(&chinese, chinese_version);
    }
}

#[test]
fn readmes_cover_security_and_known_limitations() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    for token in [
        "credentials.toml",
        "0600",
        "Portal",
        "keyring",
        "Codex",
        "Windows",
    ] {
        assert_contains(&english, token);
        assert_contains(&chinese, token);
    }
    assert_contains_ignoring_case(&english, "launch");
    assert_contains(&chinese, "启动时");
    assert_contains(&english, "injection");
    assert_contains(&chinese, "注入");
    assert_contains_ignoring_case(&english, "revocation");
    assert_contains(&chinese, "吊销");
    assert_contains_ignoring_case(&english, "hot rotation");
    assert_contains(&chinese, "热轮换");
    assert_contains_ignoring_case(&english, "project-level");
    assert_contains(&chinese, "项目级");
    assert_contains_ignoring_case(&english, "Claude live Smoke deferred");
    assert_contains(&chinese, "Claude live Smoke");
    assert_contains_ignoring_case(&english, "not signed or notarized");
    assert_contains(&chinese, "不签名");
    assert_contains(&chinese, "不公证");
}

#[test]
fn readmes_cover_uninstall_semantics() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    for token in [
        "tcamp uninstall",
        "rm \"$(command -v tcamp)\"",
        "credentials",
        "backups",
        "binary",
    ] {
        assert_contains(&english, token);
    }
    for token in [
        "tcamp uninstall",
        "rm \"$(command -v tcamp)\"",
        "凭据",
        "备份",
        "二进制",
    ] {
        assert_contains(&chinese, token);
    }
    assert_contains_ignoring_case(&english, "keeps");
    assert_contains(&chinese, "保留");
    assert_contains_ignoring_case(&english, "does not provide an updater");
    assert_contains(&chinese, "不提供 updater");
}

#[test]
fn readmes_use_glossary_terms_without_banned_synonyms() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    for banned in [
        "sign in",
        "authenticate",
        "health check",
        "e2e test",
        "integration test",
        "live test",
        "drop-in",
        "state file",
        "wrapper",
        "masking",
        "divergence",
        "named profile",
    ] {
        assert!(
            !english.to_lowercase().contains(banned),
            "README.md uses banned synonym {banned:?}"
        );
        assert!(
            !chinese.to_lowercase().contains(banned),
            "README.zh-CN.md uses banned synonym {banned:?}"
        );
    }
}

#[test]
fn changelog_is_keep_a_changelog_and_has_a_complete_v0_1_0_section() {
    let changelog = read("CHANGELOG.md");
    assert_contains(&changelog, "# Changelog");
    assert_contains(&changelog, "Keep a Changelog");
    assert_contains(&changelog, "Semantic Versioning");
    assert_contains(&changelog, "## [Unreleased]");
    assert_contains(&changelog, "## [0.1.0]");
    assert_contains(&changelog, "### Added");
    assert_contains(&changelog, "[0.1.0]:");
}
