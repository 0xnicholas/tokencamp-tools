#![cfg(unix)]
mod support;

use support::text::{assert_absent, assert_contains, assert_contains_ignoring_case, read};

const README_EN: &str = "README.md";
const README_ZH: &str = "README.zh-CN.md";

const DOCS_START: &str = "https://docs.tokencamp.com/tools/tcamp";
const DOCS_REFERENCE: &str = "https://docs.tokencamp.com/tools/tcamp/reference";
const DOCS_SECURITY: &str = "https://docs.tokencamp.com/tools/tcamp/security";

#[derive(Clone, Copy)]
struct Sections {
    documentation: &'static str,
    install: &'static str,
    quickstart: &'static str,
    configuration: &'static str,
    security: &'static str,
    license: &'static str,
}

const EN: Sections = Sections {
    documentation: "## Documentation",
    install: "## Install",
    quickstart: "## Quickstart",
    configuration: "## Configuration",
    security: "## Security",
    license: "## License",
};

const ZH: Sections = Sections {
    documentation: "## 文档",
    install: "## 安装",
    quickstart: "## 快速开始",
    configuration: "## 配置",
    security: "## 安全",
    license: "## 许可证",
};

impl Sections {
    fn headings(self) -> Vec<&'static str> {
        vec![
            self.documentation,
            self.install,
            self.quickstart,
            self.configuration,
            self.security,
            self.license,
        ]
    }
}

fn h2_headings(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.starts_with("## "))
        .collect()
}

fn section<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = text
        .find(heading)
        .unwrap_or_else(|| panic!("README is missing the {heading:?} section"));
    let body = &text[start + heading.len()..];
    match body.find("\n## ") {
        Some(end) => &body[..end],
        None => body,
    }
}

#[test]
fn readmes_are_linked_and_share_the_docs_entry_structure() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    assert_contains(&english, "[简体中文](./README.zh-CN.md)");
    assert_contains(&chinese, "[English](./README.md)");
    for text in [&english, &chinese] {
        assert_contains(text, "v0.1");
        assert_contains(text, "Claude Code");
        assert_contains(text, "Kimi Code");
        assert_contains(text, "[SPEC.md](./SPEC.md)");
        assert_contains(text, "[GLOSSARY.md](./GLOSSARY.md)");
    }

    assert_eq!(h2_headings(&english), EN.headings());
    assert_eq!(h2_headings(&chinese), ZH.headings());

    for heading in ["## Troubleshooting", "## Known limitations", "## Uninstall"] {
        assert_absent(&english, heading);
    }
    for heading in ["## 故障排查", "## 已知限制", "## 卸载"] {
        assert_absent(&chinese, heading);
    }
    for text in [&english, &chinese] {
        assert!(
            !text.lines().any(|line| line.starts_with("### ")),
            "the README entry point must not grow H3 subsections"
        );
    }
}

#[test]
fn readmes_link_the_docs_authority_from_the_top_block_and_each_retained_section() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    for (text, sections) in [(&english, EN), (&chinese, ZH)] {
        let documentation = section(text, sections.documentation);
        for url in [DOCS_START, DOCS_REFERENCE, DOCS_SECURITY] {
            assert_contains(documentation, url);
        }
        assert_contains(section(text, sections.install), DOCS_START);
        assert_contains(section(text, sections.quickstart), DOCS_START);
        assert_contains(section(text, sections.configuration), DOCS_REFERENCE);
        assert_contains(section(text, sections.security), DOCS_SECURITY);
    }
}

#[test]
fn readmes_repeat_only_the_whitelisted_install_facts() {
    let english = read(README_EN);
    let chinese = read(README_ZH);
    let english_install = section(&english, EN.install);
    let chinese_install = section(&chinese, ZH.install);

    for install in [english_install, chinese_install] {
        assert_contains(
            install,
            "https://github.com/0xnicholas/tokencamp-tools/releases/latest/download/install.sh | sh",
        );
        assert_eq!(
            install.matches("TCAMP_VERSION=v0.1.0").count(),
            1,
            "the README keeps exactly one pinned install example"
        );
        for platform in ["macOS", "Linux", "x86_64", "arm64", "Windows"] {
            assert_contains(install, platform);
        }
    }
    assert_contains_ignoring_case(english_install, "not signed or notarized");
    assert_contains_ignoring_case(english_install, "no updater");
    assert_contains_ignoring_case(english_install, "re-run the installer");
    assert_contains(chinese_install, "不签名");
    assert_contains(chinese_install, "不公证");
    assert_contains(chinese_install, "updater");
    assert_contains(chinese_install, "重新运行安装器");

    for install in [english_install, chinese_install] {
        for removed in [
            "TCAMP_INSTALL_DIR",
            "SHA256SUMS",
            "xattr",
            "Gatekeeper",
            "quarantine",
            "sh -s -- --version",
            "--version",
        ] {
            assert_absent(install, removed);
        }
    }
    assert_absent(english_install, "checksum");
    assert_absent(chinese_install, "校验和");
}

#[test]
fn readmes_keep_the_quickstart_to_four_commands_and_one_sentence() {
    let english_readme = read(README_EN);
    let chinese_readme = read(README_ZH);
    let english = section(&english_readme, EN.quickstart);
    let chinese = section(&chinese_readme, ZH.quickstart);

    for quickstart in [english, chinese] {
        for command in ["tcamp login", "tcamp claude", "tcamp pi", "tcamp kimi"] {
            assert_contains(quickstart, command);
        }
        for removed in [
            "--no-launch",
            "--dry-run",
            "--json",
            "--key-stdin",
            "TOKENCAMP_API_KEY",
            "missing_key",
            "Write it?",
            "TTY",
        ] {
            assert_absent(quickstart, removed);
        }
    }
    assert_contains_ignoring_case(english, "stores an existing key");
    assert_contains_ignoring_case(english, "Configure and Launch");
    assert_contains(chinese, "保存已有的 key");
    assert_contains(chinese, "Configure 并 Launch");
}

#[test]
fn readmes_keep_the_three_row_config_table_with_both_pi_targets() {
    let english_readme = read(README_EN);
    let chinese_readme = read(README_ZH);
    let english = section(&english_readme, EN.configuration);
    let chinese = section(&chinese_readme, ZH.configuration);

    for table in [english, chinese] {
        assert_contains(table, "| Harness | Config target | Launch |");
        assert_eq!(
            table.lines().filter(|line| line.starts_with('|')).count(),
            5,
            "the Config table keeps one header, one separator, and three rows"
        );
        for token in [
            "Claude Code",
            "Kimi Code",
            "$XDG_CONFIG_HOME/tokencamp/harnesses/claude/settings.json",
            "harnesses/claude/settings.json",
            "$PI_CODING_AGENT_DIR/models.json",
            ".pi/agent/models.json",
            ".pi/agent/settings.json",
            "$KIMI_CODE_HOME/config.toml",
            ".kimi-code/config.toml",
            "defaultProvider",
            "defaultModel",
            "claude --settings",
            "pi --provider tokencamp",
            "kimi -m",
        ] {
            assert_contains(table, token);
        }
        for removed in [
            "managed.toml",
            "Ownership manifest",
            "Drift",
            "Conflict",
            "--force",
            "--yes",
            "api_key_env",
            "providers.tokencamp",
        ] {
            assert_absent(table, removed);
        }
    }
    assert_contains(english, "writes only user-level Config targets");
    assert_contains(chinese, "只写用户级 Config target");
}

#[test]
fn readmes_summarize_security_with_the_literal_key_backup_caveat() {
    let english_readme = read(README_EN);
    let chinese_readme = read(README_ZH);
    let english = section(&english_readme, EN.security);
    let chinese = section(&chinese_readme, ZH.security);

    for security in [english, chinese] {
        for token in [
            "0600",
            "0700",
            "symlink",
            "Launch",
            "Config target",
            "Portal",
            "SECURITY.md",
            DOCS_SECURITY,
        ] {
            assert_contains(security, token);
        }
    }
    assert_contains_ignoring_case(english, "atomic");
    assert_contains_ignoring_case(english, "telemetry");
    assert_contains_ignoring_case(english, "literal key");
    assert_contains_ignoring_case(english, "backup");
    assert_contains_ignoring_case(english, "restarts");
    assert_contains_ignoring_case(english, "revocation");
    assert_contains_ignoring_case(english, "update checks");
    assert_contains(english, "pre-change backup");
    assert_absent(
        english,
        "never appears in logs, errors, diffs, JSON, or backups",
    );

    assert_contains(chinese, "原子");
    assert_contains(chinese, "遥测");
    assert_contains(chinese, "literal key");
    assert_contains(chinese, "备份");
    assert_contains(chinese, "重启");
    assert_contains(chinese, "吊销");
    assert_contains(chinese, "更新检查");
    assert_contains(chinese, "变更前备份");
    assert_absent(chinese, "日志、错误、diff、JSON 或备份");
}

#[test]
fn readmes_delegate_troubleshooting_limits_and_uninstall_to_the_docs() {
    let english = read(README_EN);
    let chinese = read(README_ZH);

    for text in [&english, &chinese] {
        for removed in [
            "tcamp uninstall",
            "rm \"$(command -v tcamp)\"",
            "401",
            "402",
            "doctor --network",
            "missing_key",
            "--no-launch",
            "--dry-run",
            "--json",
            "--force",
            "--yes",
            "Codex",
        ] {
            assert_absent(text, removed);
        }
    }
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
        "coding agent",
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
