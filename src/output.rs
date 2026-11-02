use std::env;
use std::io::{self, Write};

use anstream::{AutoStream, ColorChoice};
use anstyle::{AnsiColor, Style};
use serde::Serialize;

use crate::errors::TcampError;

const ERROR: Style = Style::new()
    .bold()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Red)));
const HINT: Style = Style::new()
    .bold()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan)));

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    ok: bool,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: String,
    hint: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<&'a str>,
}

pub fn color_choice(no_color: bool) -> ColorChoice {
    let term_is_dumb = env::var_os("TERM").is_some_and(|value| value == "dumb");
    if no_color || env::var_os("NO_COLOR").is_some() || term_is_dumb {
        ColorChoice::Never
    } else {
        ColorChoice::Auto
    }
}

pub fn report(error: &TcampError, json: bool, color: ColorChoice, verbose: bool) {
    if json {
        report_json(error);
    } else {
        report_human(error, color, verbose);
    }
}

fn report_json(error: &TcampError) {
    let envelope = ErrorEnvelope {
        ok: false,
        error: ErrorBody {
            code: error.code(),
            message: error.to_string(),
            hint: error.hint(),
            request_id: error.request_id(),
        },
    };
    if let Ok(payload) = serde_json::to_string(&envelope) {
        println!("{payload}");
    }
}

fn report_human(error: &TcampError, color: ColorChoice, verbose: bool) {
    let mut stream = AutoStream::new(io::stderr().lock(), color);
    let _ = writeln!(
        stream,
        "{ERROR}error:{ERROR:#} {} [{}]",
        error,
        error.code()
    );
    let _ = writeln!(stream, "{HINT}hint:{HINT:#} {}", error.hint());
    if verbose {
        if let Some(request_id) = error.request_id() {
            let _ = writeln!(stream, "request_id: {request_id}");
        }
    }
}
