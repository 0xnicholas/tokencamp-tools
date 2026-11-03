use std::path::Path;

const CONTEXT_LINES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit<'a> {
    Equal(&'a str),
    Delete(&'a str),
    Insert(&'a str),
}

pub fn unified_diff(current: Option<&[u8]>, desired: &[u8], path: &Path) -> String {
    let old = current.map(|bytes| String::from_utf8_lossy(bytes).into_owned());
    let new = String::from_utf8_lossy(desired);
    let old_lines: Vec<&str> = old.as_deref().map(split_lines).unwrap_or_default();
    let new_lines = split_lines(&new);

    let edits = edit_script(&old_lines, &new_lines);
    let mut output = String::new();
    output.push_str(&format!("--- {}\n", path.display()));
    output.push_str(&format!("+++ {}\n", path.display()));

    for hunk in hunks(&edits) {
        output.push_str(&render_hunk(&hunk));
    }
    output
}

const SECRET_KEYS: [&str; 5] = [
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "api_key",
    "apiKey",
    "auth_token",
];

pub fn redact(text: &str) -> String {
    let line = redact_sk_tokens(text);
    SECRET_KEYS
        .iter()
        .fold(line, |line, key| mask_secret_value(&line, key))
}

fn redact_sk_tokens(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("sk-") {
        output.push_str(&rest[..index]);
        let after = &rest[index + 3..];
        let token_len = after
            .find(|character: char| {
                !(character.is_ascii_alphanumeric() || character == '_' || character == '-')
            })
            .unwrap_or(after.len());
        if token_len >= 8 {
            output.push_str("sk-…");
            output.push_str(&after[token_len - 4..token_len]);
            rest = &after[token_len..];
        } else {
            output.push_str("sk-");
            rest = after;
        }
    }
    output.push_str(rest);
    output
}

fn mask_secret_value(line: &str, key: &str) -> String {
    let Some(key_index) = line.find(key) else {
        return line.to_string();
    };
    let after = &line[key_index + key.len()..];
    let Some(separator) = after.find([':', '=']) else {
        return line.to_string();
    };
    let value_part = &after[separator + 1..];
    let Some(open) = value_part.find('"') else {
        return line.to_string();
    };
    let value_start = key_index + key.len() + separator + 1 + open + 1;
    let rest = &line[value_start..];
    let Some(close) = rest.find('"') else {
        return line.to_string();
    };
    let value = &rest[..close];
    if value.is_empty() {
        return line.to_string();
    }
    format!(
        "{}{}{}",
        &line[..value_start],
        mask_secret(value),
        &line[value_start + close..]
    )
}

fn mask_secret(value: &str) -> String {
    let characters: Vec<char> = value.chars().collect();
    if characters.len() < 8 {
        return "…".to_string();
    }
    let suffix: String = characters[characters.len() - 4..].iter().collect();
    format!("sk-…{suffix}")
}

fn split_lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn edit_script<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Edit<'a>> {
    let mut table = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    let mut edits = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            edits.push(Edit::Equal(old[i]));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            edits.push(Edit::Delete(old[i]));
            i += 1;
        } else {
            edits.push(Edit::Insert(new[j]));
            j += 1;
        }
    }
    while i < old.len() {
        edits.push(Edit::Delete(old[i]));
        i += 1;
    }
    while j < new.len() {
        edits.push(Edit::Insert(new[j]));
        j += 1;
    }
    edits
}

struct Hunk<'a> {
    old_start: usize,
    old_len: usize,
    new_start: usize,
    new_len: usize,
    lines: Vec<(char, &'a str)>,
}

fn hunks<'a>(edits: &[Edit<'a>]) -> Vec<Hunk<'a>> {
    let changed: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter(|(_, edit)| !matches!(edit, Edit::Equal(_)))
        .map(|(index, _)| index)
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for index in changed {
        let start = index.saturating_sub(CONTEXT_LINES);
        let end = (index + CONTEXT_LINES + 1).min(edits.len());
        match ranges.last_mut() {
            Some((_, last_end)) if start <= *last_end => *last_end = (*last_end).max(end),
            _ => ranges.push((start, end)),
        }
    }

    let mut old_consumed = 0;
    let mut new_consumed = 0;
    let mut position = 0;
    let mut result = Vec::new();
    for (start, end) in ranges {
        while position < start {
            match edits[position] {
                Edit::Equal(_) => {
                    old_consumed += 1;
                    new_consumed += 1;
                }
                Edit::Delete(_) => old_consumed += 1,
                Edit::Insert(_) => new_consumed += 1,
            }
            position += 1;
        }

        let mut old_len = 0;
        let mut new_len = 0;
        let mut lines = Vec::new();
        while position < end {
            match edits[position] {
                Edit::Equal(line) => {
                    old_len += 1;
                    new_len += 1;
                    lines.push((' ', line));
                }
                Edit::Delete(line) => {
                    old_len += 1;
                    lines.push(('-', line));
                }
                Edit::Insert(line) => {
                    new_len += 1;
                    lines.push(('+', line));
                }
            }
            position += 1;
        }

        result.push(Hunk {
            old_start: if old_len == 0 {
                old_consumed
            } else {
                old_consumed + 1
            },
            old_len,
            new_start: if new_len == 0 {
                new_consumed
            } else {
                new_consumed + 1
            },
            new_len,
            lines,
        });

        old_consumed += old_len;
        new_consumed += new_len;
    }
    result
}

fn render_hunk(hunk: &Hunk<'_>) -> String {
    let mut output = format!(
        "@@ -{},{} +{},{} @@\n",
        hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len
    );
    for (kind, line) in &hunk.lines {
        output.push(*kind);
        output.push_str(&redact(line));
        output.push('\n');
    }
    output
}
