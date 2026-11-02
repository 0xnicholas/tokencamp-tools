use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::ColorChoice as ClapColorChoice;
use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand};

use crate::errors::TcampError;
use crate::harness::{self, Harness};
use crate::output;

const QUICKSTART: &str = "Quickstart:
  tcamp login
  tcamp <harness>

Harnesses: claude, pi, kimi";

#[derive(Debug, Parser)]
#[command(
    name = "tcamp",
    version,
    about = "Configure and launch coding agent harnesses against tokencamp",
    after_help = QUICKSTART,
    color = ClapColorChoice::Never,
    arg_required_else_help = false,
    subcommand_required = false
)]
pub struct Cli {
    #[arg(short = 'v', long, global = true, help = "Show verbose diagnostics")]
    pub verbose: bool,

    #[arg(long, global = true, help = "Disable colored output")]
    pub no_color: bool,

    #[arg(long, global = true, help = "Emit one JSON object and no human output")]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Configure and launch Claude Code
    Claude(HarnessArgs),

    /// Configure and launch Pi
    Pi(HarnessArgs),

    /// Configure and launch Kimi Code
    Kimi(HarnessArgs),

    /// Store and verify an existing tokencamp API key
    Login(LoginArgs),

    /// Remove a stored tokencamp credential
    Logout(LogoutArgs),

    /// Show the local connection and harness snapshot
    Status,

    /// Check the local setup and optional network connectivity
    Doctor(DoctorArgs),

    /// Remove the configuration entries managed by tcamp
    Uninstall(UninstallArgs),
}

#[derive(Debug, Args)]
pub struct HarnessArgs {
    /// Configure without launching the harness
    #[arg(long)]
    pub no_launch: bool,

    /// Show the pending configuration changes without writing
    #[arg(long)]
    pub dry_run: bool,

    /// Override the main model for this launch only
    #[arg(long, value_name = "ID")]
    pub model: Option<String>,

    /// Override the tokencamp base URL
    #[arg(long, value_name = "URL")]
    pub base_url: Option<String>,

    /// Adopt entries that tcamp does not own
    #[arg(long)]
    pub force: bool,

    /// Answer confirmation prompts with yes
    #[arg(long)]
    pub yes: bool,

    /// Arguments passed through to the harness
    #[arg(last = true, value_name = "ARGS", allow_hyphen_values = true)]
    pub harness_args: Vec<OsString>,
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// Read the API key from stdin
    #[arg(long)]
    pub key_stdin: bool,

    /// Save the API key without any network check
    #[arg(long, conflicts_with = "verify")]
    pub no_verify: bool,

    /// Send one minimal chat request after the auth probe
    #[arg(long)]
    pub verify: bool,

    /// Replace an existing API key without prompting
    #[arg(long)]
    pub force: bool,

    /// Base URL to log in to
    #[arg(long, value_name = "URL")]
    pub base_url: Option<String>,
}

#[derive(Debug, Args)]
pub struct LogoutArgs {
    /// Remove credentials for every stored connection
    #[arg(long)]
    pub all: bool,

    /// Connection whose credential should be removed
    #[arg(long, value_name = "URL", conflicts_with = "all")]
    pub base_url: Option<String>,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Run the free network checks
    #[arg(long)]
    pub network: bool,

    /// Send at most one minimal real request
    #[arg(long)]
    pub verify: bool,

    /// Limit harness checks to one harness
    #[arg(long, value_name = "HARNESS")]
    pub harness: Option<String>,
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// Limit uninstall to this harness (repeatable)
    #[arg(long, value_name = "HARNESS")]
    pub harness: Vec<String>,

    /// Show the uninstall plan without writing
    #[arg(long)]
    pub dry_run: bool,

    /// Answer the confirmation prompt with yes
    #[arg(long)]
    pub yes: bool,

    /// Remove entries that were modified outside tcamp
    #[arg(long)]
    pub force: bool,
}

pub fn run() -> ExitCode {
    run_with_args(std::env::args_os().collect())
}

fn run_with_args(args: Vec<OsString>) -> ExitCode {
    let json_requested = has_flag(&args, "--json");
    let no_color = has_flag(&args, "--no-color");
    let parse_color = output::color_choice(no_color);

    match Cli::try_parse_from(&args) {
        Ok(cli) => {
            let json = cli.json;
            let verbose = cli.verbose;
            let color = output::color_choice(cli.no_color);
            match dispatch(cli) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    output::report(&error, json, color, verbose);
                    ExitCode::from(error.exit_code())
                }
            }
        }
        Err(error) => handle_parse_error(&args, error, json_requested, parse_color),
    }
}

fn dispatch(cli: Cli) -> Result<(), TcampError> {
    let Some(command) = cli.command else {
        if cli.json {
            return Err(TcampError::usage(
                "`--json` requires a command",
                "run `tcamp help` to see available commands",
            ));
        }
        print_bare_help();
        return Ok(());
    };

    match &command {
        Command::Claude(args) | Command::Pi(args) | Command::Kimi(args) => {
            let name = command_name(&command);
            if cli.json && !args.no_launch && !args.dry_run {
                return Err(TcampError::usage(
                    format!("`--json` cannot be combined with launching `{name}`"),
                    format!(
                        "use `tcamp {name} --no-launch --json` or `tcamp {name} --dry-run --json`"
                    ),
                ));
            }
        }
        _ => {}
    }

    Err(TcampError::not_implemented(command_name(&command)))
}

fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Claude(_) => Harness::Claude.id(),
        Command::Pi(_) => Harness::Pi.id(),
        Command::Kimi(_) => Harness::Kimi.id(),
        Command::Login(_) => "login",
        Command::Logout(_) => "logout",
        Command::Status => "status",
        Command::Doctor(_) => "doctor",
        Command::Uninstall(_) => "uninstall",
    }
}

fn print_bare_help() {
    let mut command = Cli::command();
    let mut stdout = io::stdout().lock();
    let _ = command.write_help(&mut stdout);
    let _ = writeln!(stdout);

    let detected = harness::detect();
    if detected.is_empty() {
        let _ = writeln!(stdout, "Detected harnesses: none");
    } else {
        let names: Vec<&str> = detected.iter().map(|harness| harness.id()).collect();
        let _ = writeln!(stdout, "Detected harnesses: {}", names.join(", "));
    }
}

fn handle_parse_error(
    args: &[OsString],
    error: clap::Error,
    json: bool,
    color: anstream::ColorChoice,
) -> ExitCode {
    match error.kind() {
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayVersion
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            let _ = error.print();
            ExitCode::SUCCESS
        }
        _ => {
            let message = first_error_line(&error);
            let hint = parse_hint(args, &error);
            let error = TcampError::usage(message, hint);
            output::report(&error, json, color, false);
            ExitCode::from(error.exit_code())
        }
    }
}

fn first_error_line(error: &clap::Error) -> String {
    let rendered = error.to_string();
    let line = rendered
        .lines()
        .next()
        .unwrap_or("invalid command line")
        .trim();
    line.strip_prefix("error: ").unwrap_or(line).to_string()
}

fn parse_hint(args: &[OsString], error: &clap::Error) -> String {
    match error.kind() {
        ErrorKind::InvalidSubcommand => {
            let subcommand = first_subcommand(args).unwrap_or_default();
            if subcommand == "codex" {
                "Codex is not supported in v1; supported harnesses are claude, pi, and kimi"
                    .to_string()
            } else {
                "run `tcamp help` to see available commands".to_string()
            }
        }
        ErrorKind::UnknownArgument if harness_subcommand(args).is_some() => {
            "unknown flags before `--` are rejected; pass harness arguments after `--`".to_string()
        }
        _ => "run `tcamp help` to see available commands and options".to_string(),
    }
}

fn first_subcommand(args: &[OsString]) -> Option<&str> {
    args.iter()
        .skip(1)
        .map(|arg| arg.to_str())
        .take_while(|arg| *arg != Some("--"))
        .flatten()
        .find(|arg| !arg.starts_with('-'))
}

fn harness_subcommand(args: &[OsString]) -> Option<Harness> {
    first_subcommand(args).and_then(Harness::from_id)
}

fn has_flag(args: &[OsString], flag: &str) -> bool {
    args.iter()
        .skip(1)
        .take_while(|arg| arg.to_str() != Some("--"))
        .any(|arg| arg.to_str() == Some(flag))
}
