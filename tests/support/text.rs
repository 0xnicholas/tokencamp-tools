#![cfg(unix)]

use std::fs;

use super::dist;

pub fn read(relative: &str) -> String {
    fs::read_to_string(dist::repo_root().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

pub fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected text to contain {needle:?}:\n{haystack}"
    );
}

pub fn assert_contains_ignoring_case(haystack: &str, needle: &str) {
    assert!(
        haystack.to_lowercase().contains(&needle.to_lowercase()),
        "expected text to contain {needle:?} (case-insensitive)"
    );
}

pub fn assert_absent(haystack: &str, needle: &str) {
    assert!(
        !haystack.contains(needle),
        "expected text not to contain {needle:?}"
    );
}
