#![cfg(unix)]
mod support;

use std::fs;
use std::path::Path;

use support::dist;
use support::text::{assert_absent, assert_contains, read};

const TARGETS: [&str; 4] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
];

fn top_level_block(text: &str, key: &str) -> String {
    let header = format!("{key}:");
    let mut block = String::new();
    let mut inside = false;
    for line in text.lines() {
        let is_top_level = !line.is_empty() && !line.starts_with(' ') && !line.starts_with('#');
        if inside && is_top_level {
            break;
        }
        if !inside && line == header {
            inside = true;
        }
        if inside {
            block.push_str(line);
            block.push('\n');
        }
    }
    assert!(inside, "missing top-level `{key}:` block");
    block
}

fn job_block(workflow: &str, job: &str) -> String {
    let jobs = top_level_block(workflow, "jobs");
    let mut block = String::new();
    let mut inside = false;
    for line in jobs.lines().skip(1) {
        let is_job_header =
            line.starts_with("  ") && !line.starts_with("    ") && line.ends_with(':');
        if is_job_header {
            if inside {
                break;
            }
            if line.trim().trim_end_matches(':') == job {
                inside = true;
            }
        }
        if inside {
            block.push_str(line);
            block.push('\n');
        }
    }
    assert!(inside, "missing job `{job}`");
    block
}

fn workflow_files() -> Vec<(String, String)> {
    let dir = dist::repo_root().join(".github/workflows");
    let mut files = Vec::new();
    for entry in fs::read_dir(&dir).expect("read workflows directory") {
        let path = entry.expect("workflow entry").path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("yml")
            || path.extension().and_then(|extension| extension.to_str()) == Some("yaml")
        {
            let name = path
                .file_name()
                .expect("workflow file name")
                .to_string_lossy()
                .into_owned();
            files.push((name, fs::read_to_string(&path).expect("read workflow")));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    assert!(!files.is_empty(), "no workflow files found");
    files
}

#[test]
fn release_workflow_is_triggered_only_by_version_tags() {
    let workflow = read(".github/workflows/release.yml");
    let triggers = top_level_block(&workflow, "on");

    assert_contains(&triggers, "push:");
    assert_contains(&triggers, "tags:");
    assert!(
        triggers
            .lines()
            .any(|line| matches!(line.trim(), "- \"v*\"" | "- 'v*'" | "- v*")),
        "release workflow must trigger on v* tags:\n{triggers}"
    );
    for forbidden in [
        "pull_request",
        "workflow_dispatch",
        "schedule",
        "branches",
        "workflow_call",
    ] {
        assert_absent(&triggers, forbidden);
    }
}

#[test]
fn release_workflow_verifies_and_builds_every_target_with_release_gates() {
    let workflow = read(".github/workflows/release.yml");
    let verify = job_block(&workflow, "verify");
    assert_contains(&verify, "runs-on: ubuntu-latest");
    assert_contains(&verify, "cargo fmt --all -- --check");
    assert_contains(
        &verify,
        "cargo clippy --all-targets --locked -- -D warnings",
    );
    assert_contains(&verify, "cargo test --locked");

    let build = job_block(&workflow, "build");
    assert_contains(&build, "needs: verify");
    for target in TARGETS {
        assert_contains(&build, target);
    }
    assert_contains(&build, "MACOSX_DEPLOYMENT_TARGET");
    assert_contains(&build, "11.0");
    assert_contains(&build, "cargo build --release --locked --target");
    assert_contains(&build, "target/$TARGET/release/tcamp --version");
    assert_contains(&build, "tcamp $version");
    assert_contains(&build, "scripts/package.sh");
    assert_contains(&build, "scripts/check-release-gates.sh");
    assert_contains(&build, "actions/upload-artifact@v");
    assert_absent(&build, "continue-on-error");
    assert_absent(&build, "cargo publish");
    assert_absent(&build, "npm publish");
}

#[test]
fn release_workflow_publishes_only_from_main_with_changelog_notes() {
    let workflow = read(".github/workflows/release.yml");
    let publish = job_block(&workflow, "publish");
    assert_contains(&publish, "needs: build");
    assert_contains(&publish, "fetch-depth: 0");
    assert_contains(&publish, "git fetch --no-tags origin main");
    assert_contains(&publish, "git merge-base --is-ancestor");
    assert_contains(&publish, "origin/main");
    assert_contains(&publish, "CHANGELOG.md");
    assert_contains(&publish, "## [$version]");
    assert_contains(&publish, "exit 1");

    assert_contains(&publish, "gh release create");
    assert_contains(&publish, "--verify-tag");
    assert_contains(&publish, "--draft");
    assert_contains(&publish, "gh release upload");
    assert_contains(&publish, "dist/tcamp-$version-*.tar.gz");
    assert_contains(&publish, "dist/tcamp-$version-*.tar.gz.sha256");
    assert_contains(&publish, "SHA256SUMS");
    assert_contains(&publish, "install.sh");
    assert_contains(&publish, "case \"$GITHUB_REF_NAME\" in");
    assert_contains(&publish, "*-*");
    assert_contains(&publish, "--prerelease");
    assert_contains(&publish, "gh release edit");
    assert_contains(&publish, "--draft=false");
    assert_contains(&publish, "actions/attest-build-provenance@v");
    assert_contains(&publish, "subject-path:");
    assert_contains(&publish, "dist/*.tar.gz.sha256");
    assert_contains(&publish, "immutable-releases");
}

#[test]
fn workflows_use_minimal_permissions_and_never_publish_to_package_registries() {
    let release = read(".github/workflows/release.yml");
    let top_permissions = top_level_block(&release, "permissions");
    assert_contains(&top_permissions, "contents: read");

    let concurrency = top_level_block(&release, "concurrency");
    assert_contains(&concurrency, "release-${{ github.ref }}");
    assert_contains(&concurrency, "cancel-in-progress: false");

    let publish = job_block(&release, "publish");
    for permission in ["contents: write", "id-token: write", "attestations: write"] {
        assert_contains(&publish, permission);
    }
    for job in ["verify", "build"] {
        assert_absent(&job_block(&release, job), "write");
    }

    for (name, workflow) in workflow_files() {
        for forbidden in [
            "cargo publish",
            "npm publish",
            "NPM_TOKEN",
            "CARGO_REGISTRY_TOKEN",
            "registry-url:",
        ] {
            assert!(
                !workflow.contains(forbidden),
                "{name} must not contain {forbidden:?}"
            );
        }
        for line in workflow.lines().filter(|line| line.contains("uses:")) {
            assert!(
                line.contains("@v") || line.contains("dtolnay/rust-toolchain@"),
                "{name} must use a major tag for actions: {line}"
            );
        }
    }
}

#[test]
fn ci_covers_msrv_supply_chain_and_dependabot_contracts() {
    let ci = read(".github/workflows/ci.yml");
    let test_job = job_block(&ci, "test");
    assert_contains(&test_job, "ubuntu-latest");
    assert_contains(&test_job, "macos-15");
    assert_contains(&ci, "macos-14 jobs are cancelled");
    assert_contains(&test_job, "cargo fmt --all -- --check");
    assert_contains(
        &test_job,
        "cargo clippy --all-targets --locked -- -D warnings",
    );
    assert_contains(&test_job, "cargo test --locked");

    let msrv = job_block(&ci, "msrv");
    assert_contains(&msrv, "1.85");
    assert_contains(&msrv, "cargo");
    assert_contains(&msrv, "--locked");

    let ci_permissions = top_level_block(&ci, "permissions");
    assert_contains(&ci_permissions, "contents: read");
    assert_absent(&ci, "contents: write");

    let deny = read(".github/workflows/deny.yml");
    let deny_triggers = top_level_block(&deny, "on");
    assert_contains(&deny_triggers, "schedule:");
    assert_contains(&deny_triggers, "cron:");
    assert_contains(&deny_triggers, "workflow_dispatch:");
    assert_contains(&deny, "EmbarkStudios/cargo-deny-action@v");
    assert_contains(&deny, "advisories");
    assert_contains(&deny, "licenses");
    let deny_permissions = top_level_block(&deny, "permissions");
    assert_contains(&deny_permissions, "contents: read");

    let deny_config = read("deny.toml");
    assert_contains(&deny_config, "[advisories]");
    assert_contains(&deny_config, "[licenses]");
    assert_contains(&deny_config, "\"Apache-2.0\"");
    assert_contains(&deny_config, "\"MIT\"");

    let dependabot = read(".github/dependabot.yml");
    assert_contains(&dependabot, "version: 2");
    assert_contains(&dependabot, "package-ecosystem: cargo");
    assert_contains(&dependabot, "package-ecosystem: github-actions");
    assert_eq!(
        dependabot.matches("interval: weekly").count(),
        2,
        "both ecosystems must update weekly"
    );
    assert_eq!(
        dependabot.matches("groups:").count(),
        2,
        "both ecosystems must use grouped updates"
    );
}

#[test]
fn ci_records_latency_without_blocking_the_build() {
    let ci = read(".github/workflows/ci.yml");
    let latency = job_block(&ci, "latency");
    assert_contains(&latency, "runs-on: ubuntu-latest");
    assert_contains(&latency, "continue-on-error: true");
    assert_contains(&latency, "cargo build --release --locked");
    assert_contains(&latency, "scripts/package.sh");
    assert_contains(&latency, "scripts/check-release-gates.sh");
    assert_contains(&latency, "TCAMP_STARTUP_P95_LIMIT_MS=10000");
    assert_contains(&latency, "TCAMP_OFFLINE_P95_LIMIT_MS=10000");
    assert_contains(&latency, "TCAMP_LATENCY_SAMPLES=10");
    assert_contains(&latency, "shellcheck");
    assert_contains(&latency, "sh -n scripts/check-release-gates.sh");
    assert_absent(&latency, "exit 1");
    assert_absent(&latency, "::error::");
}

#[test]
fn release_immutability_check_never_fails_the_release() {
    let workflow = read(".github/workflows/release.yml");
    let publish = job_block(&workflow, "publish");
    let start = publish
        .find("Check release immutability")
        .expect("publish must check release immutability");
    let rest = &publish[start..];
    let end = rest
        .find("Create draft release")
        .expect("immutability check must precede the release");
    let step = &rest[..end];

    assert_contains(step, "immutable-releases");
    assert_contains(step, "::warning::");
    assert_contains(step, "Could not verify release immutability");
    assert_absent(step, "::error::");
    assert_absent(step, "exit 1");
}

#[test]
fn release_assets_are_only_the_contract_files() {
    let workflow = read(".github/workflows/release.yml");
    let publish = job_block(&workflow, "publish");

    assert_contains(&publish, "find dist");
    assert_contains(&publish, "-maxdepth 1");
    assert_contains(&publish, "-name '*.tar.gz'");
    assert_contains(&publish, "-eq 4");
    assert_contains(&publish, "cat *.tar.gz.sha256");
    assert_contains(&publish, "sha256sum -c");
}

#[test]
fn repository_has_no_contributing_or_template_files() {
    let root = dist::repo_root();
    for path in [
        "CONTRIBUTING.md",
        ".github/ISSUE_TEMPLATE",
        ".github/PULL_REQUEST_TEMPLATE.md",
        ".github/pull_request_template.md",
    ] {
        assert!(!root.join(path).exists(), "{path} must not exist in v1");
    }
}

#[test]
fn security_policy_only_points_to_private_vulnerability_reporting() {
    let security = read("SECURITY.md");
    assert_contains(&security, "GitHub Private Vulnerability Reporting");
    assert_contains(
        &security,
        "https://github.com/0xnicholas/tokencamp-tools/security/advisories/new",
    );
    assert_absent(&security, "mailto:");
    assert_absent(&security, "security@");
}

#[test]
fn changelog_has_a_v0_1_0_section_for_the_release_workflow() {
    let changelog = read("CHANGELOG.md");
    assert_contains(&changelog, "Keep a Changelog");
    assert!(
        changelog.lines().any(|line| line.starts_with("## [0.1.0]")),
        "CHANGELOG.md must contain a `## [0.1.0]` section"
    );
    assert_contains(&changelog, "### Added");
    assert!(
        !changelog.contains("## [v0.1.0]"),
        "release headings must use the version without the v prefix"
    );
}

#[test]
fn docs_sources_do_not_carry_per_file_license_headers() {
    let root = dist::repo_root();
    for directory in ["src", "tests", "scripts"] {
        check_no_license_headers(&root.join(directory));
    }
}

fn check_no_license_headers(path: &Path) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("read source directory") {
            check_no_license_headers(&entry.expect("source entry").path());
        }
        return;
    }
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return;
    };
    if !matches!(extension, "rs" | "sh") {
        return;
    }
    let contents = fs::read_to_string(path).expect("read source file");
    for marker in ["SPDX-License-Identifier", "Copyright"] {
        assert!(
            !contents.lines().take(5).any(|line| line.contains(marker)),
            "{} must not carry a per-file license header ({marker})",
            path.display()
        );
    }
}
