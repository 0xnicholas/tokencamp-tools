use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::ColorChoice as ClapColorChoice;
use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand};
use serde::Serialize;

use crate::config;
use crate::credentials::{self, ConnectionContext, KeySource, LoginReport, LogoutReport, Warning};
use crate::errors::TcampError;
use crate::harness::{self, Harness, claude, kimi, pi};
use crate::models;
use crate::output;
use crate::status;
use crate::terminal;
use crate::uninstall;

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
                Ok(code) => ExitCode::from(code),
                Err(error) => {
                    output::report(&error, json, color, verbose);
                    ExitCode::from(error.exit_code())
                }
            }
        }
        Err(error) => handle_parse_error(&args, error, json_requested, parse_color),
    }
}

fn dispatch(cli: Cli) -> Result<u8, TcampError> {
    let Some(command) = cli.command else {
        if cli.json {
            return Err(TcampError::usage(
                "`--json` requires a command",
                "run `tcamp help` to see available commands",
            ));
        }
        print_bare_help();
        return Ok(0);
    };

    if let Command::Claude(args) | Command::Pi(args) | Command::Kimi(args) = &command {
        let name = command_name(&command);
        if cli.json && !args.no_launch && !args.dry_run {
            return Err(TcampError::usage(
                format!("`--json` cannot be combined with launching `{name}`"),
                format!("use `tcamp {name} --no-launch --json` or `tcamp {name} --dry-run --json`"),
            ));
        }
    }

    match command {
        Command::Login(args) => {
            match credentials::login(args, cli.json)? {
                Some(report) => print_login_report(report, cli.json)?,
                None => {
                    if !cli.json {
                        println!("No changes.");
                    }
                }
            }
            Ok(0)
        }
        Command::Logout(args) => {
            print_logout_report(credentials::logout(args)?, cli.json)?;
            Ok(0)
        }
        Command::Status => {
            print_status_report(status::snapshot()?, cli.json)?;
            Ok(0)
        }
        Command::Claude(args) => {
            configure_claude(args, cli.json)?;
            Ok(0)
        }
        Command::Pi(args) => {
            configure_pi(args, cli.json)?;
            Ok(0)
        }
        Command::Kimi(args) => {
            configure_kimi(args, cli.json)?;
            Ok(0)
        }
        Command::Doctor(args) => crate::doctor::run(
            args,
            cli.json,
            cli.verbose,
            output::color_choice(cli.no_color),
        ),
        Command::Uninstall(args) => {
            uninstall::run(args, cli.json)?;
            Ok(0)
        }
    }
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

#[derive(Serialize)]
struct SuccessReport {
    ok: bool,
    base_url: Option<String>,
    key: Option<KeyReport>,
    harnesses: BTreeMap<String, HarnessReport>,
    warnings: Vec<WarningReport>,
}

#[derive(Serialize)]
struct HarnessReport {
    configured: bool,
    target: String,
    action: &'static str,
}

#[derive(Serialize)]
struct ConfigureReport {
    ok: bool,
    base_url: Option<String>,
    key: Option<KeyReport>,
    harnesses: BTreeMap<String, HarnessReport>,
    changes: Vec<ChangeReport>,
    warnings: Vec<WarningReport>,
}

#[derive(Serialize)]
struct ChangeReport {
    path: String,
    action: &'static str,
    diff: String,
}

#[derive(Serialize)]
struct KeyReport {
    masked: String,
    source: &'static str,
    verified: bool,
    verified_at: Option<String>,
}

#[derive(Serialize)]
struct WarningReport {
    code: String,
    message: String,
}

fn warning_reports(warnings: &[Warning]) -> Vec<WarningReport> {
    warnings
        .iter()
        .map(|warning| WarningReport {
            code: warning.code.clone(),
            message: warning.message.clone(),
        })
        .collect()
}

fn key_report(
    masked: Option<String>,
    source: Option<KeySource>,
    verified_at: Option<String>,
) -> Option<KeyReport> {
    match (masked, source) {
        (Some(masked), Some(source)) => Some(KeyReport {
            masked,
            source: source.as_str(),
            verified: verified_at.is_some(),
            verified_at,
        }),
        _ => None,
    }
}

fn harness_report(
    harness: &str,
    target: &std::path::Path,
    action: &'static str,
    configured: bool,
) -> BTreeMap<String, HarnessReport> {
    let mut harnesses = BTreeMap::new();
    harnesses.insert(
        harness.to_string(),
        HarnessReport {
            configured,
            target: target.display().to_string(),
            action,
        },
    );
    harnesses
}

fn print_dry_run(
    connection: ConnectionContext,
    harness: &str,
    target: std::path::PathBuf,
    action: &'static str,
    changes: Vec<ChangeReport>,
    warnings: Vec<Warning>,
    json: bool,
) -> Result<(), TcampError> {
    let harnesses = harness_report(harness, &target, action, action != "create");

    if json {
        return output::print_json(&ConfigureReport {
            ok: true,
            base_url: Some(connection.base_url),
            key: key_report(
                connection.masked_key,
                connection.source,
                connection.verified_at,
            ),
            harnesses,
            changes,
            warnings: warning_reports(&warnings),
        });
    }

    let diff = changes
        .iter()
        .map(|change| change.diff.as_str())
        .collect::<String>();
    if diff.is_empty() {
        println!("No changes.");
    } else {
        print!("{diff}");
    }
    eprintln!("no files written");
    print_warnings(&warnings);
    Ok(())
}

fn configure_claude(args: HarnessArgs, json: bool) -> Result<(), TcampError> {
    let mut connection = credentials::resolve_connection(args.base_url.as_deref())?;
    let selection = models::resolve_claude_model(args.model.as_deref())?;
    if !args.no_launch && !args.dry_run {
        connection = credentials::ensure_launch_key(connection, terminal::is_interactive())?;
    }
    let connection = connection.require_key()?;
    let target = config::claude_overlay_path()?;
    let desired_text = claude::desired_overlay(&connection.base_url, models::DEFAULT_MAIN_MODEL);
    let desired = move |_existing: &config::TargetContents| Ok(desired_text.clone().into_bytes());
    let model_ids = models::claude_model_ids(&selection.main);

    let mut warnings = connection.warnings.clone();
    if !args.dry_run && !args.no_launch {
        let _ = harness::launch::require_binary(Harness::Claude)?;
    }

    let shadowing = claude::inspect_shadowing();
    if args.dry_run || args.no_launch {
        for conflict in &shadowing.conflicts {
            warnings.push(Warning::new("shadowing_conflict", conflict.message()));
        }
        for problem in &shadowing.unreadable {
            warnings.push(Warning::new("shadowing_unreadable", problem.message()));
        }
        warnings.extend(shadowing.warnings);
    } else {
        if !shadowing.conflicts.is_empty() {
            let message = shadowing
                .conflicts
                .iter()
                .map(|conflict| conflict.message())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(TcampError::config_conflict(
                message,
                "remove the setting from the Claude settings file, or use a separate Claude profile; `--force` does not bypass shadowing",
            ));
        }
        if !shadowing.unreadable.is_empty() {
            let message = shadowing
                .unreadable
                .iter()
                .map(|problem| problem.message())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(TcampError::config_unreadable(
                message,
                "make the Claude settings readable or remove them and try again; tcamp cannot rule out credential shadowing",
            ));
        }
        warnings.extend(shadowing.warnings);
    }

    if !args.dry_run {
        let existing = config::read_target(&target)?;
        if let Some(contents) = existing.contents.as_deref() {
            let ownership = claude::overlay_ownership(contents, &target)?;
            if ownership == claude::OverlayOwnership::Unowned && !args.force {
                return Err(claude::unowned_conflict(&target));
            }
        }
        config::manifest::Manifest::load()?;

        let api_key = credentials::require_launch_key(&connection.base_url)?;
        let model_check =
            harness::preflight::check_models(&connection.base_url, &api_key, &model_ids);
        if !model_check.missing.is_empty() {
            return Err(TcampError::usage(
                format!(
                    "the model(s) {} were not found at {}",
                    model_check.missing.join(", "),
                    connection.base_url
                ),
                "pass a different main model with `--model`, or upgrade tcamp when the curated catalog changes",
            ));
        }
        warnings.extend(model_check.warnings);
    }

    let target_for_inspect = target.clone();
    let force = args.force;
    let inspect = move |existing: &config::TargetContents,
                        entry: Option<&config::manifest::ManifestEntry>|
          -> Result<config::engine::Inspection, TcampError> {
        let ownership = existing
            .contents
            .as_deref()
            .map(|contents| claude::overlay_ownership(contents, &target_for_inspect))
            .transpose()?;

        if ownership == Some(claude::OverlayOwnership::Unowned) && !force {
            return Err(claude::unowned_conflict(&target_for_inspect));
        }

        let drift = ownership == Some(claude::OverlayOwnership::Owned)
            && entry
                .and_then(|entry| entry.last_written_hash.as_deref())
                .zip(existing.contents.as_deref())
                .is_some_and(|(previous, contents)| {
                    previous != config::manifest::hash_bytes(contents)
                });

        Ok(config::engine::Inspection {
            drift,
            adopted: ownership == Some(claude::OverlayOwnership::Unowned) && force,
            model_ids: models::claude_model_ids(models::DEFAULT_MAIN_MODEL),
            provider_entry: None,
            defaults: std::collections::BTreeMap::new(),
        })
    };

    let prompt = !json && !args.yes && terminal::is_tty();
    let outcome = config::engine::converge(config::engine::Request {
        harness: claude::HARNESS_ID,
        target: target.clone(),
        desired: &desired,
        dry_run: args.dry_run,
        prompt,
        inspect: &inspect,
    })?;

    warnings.extend(outcome.warnings);

    if args.dry_run {
        let action = outcome.action.as_str();
        return print_dry_run(
            connection,
            claude::HARNESS_ID,
            target.clone(),
            action,
            vec![ChangeReport {
                path: target.display().to_string(),
                action,
                diff: outcome.diff.unwrap_or_default(),
            }],
            warnings,
            json,
        );
    }

    let harnesses = harness_report(claude::HARNESS_ID, &target, outcome.action.as_str(), true);

    if json {
        return output::print_json(&SuccessReport {
            ok: true,
            base_url: Some(connection.base_url),
            key: key_report(
                connection.masked_key,
                connection.source,
                connection.verified_at,
            ),
            harnesses,
            warnings: warning_reports(&warnings),
        });
    }

    print_warnings(&warnings);

    if outcome.declined {
        println!("No changes.");
        return Ok(());
    }

    if args.no_launch {
        if outcome.action == config::engine::Action::None {
            println!("No changes.");
        } else {
            println!("Configured claude: {}", target.display());
        }
        return Ok(());
    }

    if outcome.action != config::engine::Action::None {
        eprintln!("Configured claude: {}", target.display());
    }
    launch_claude(&args, &target, &connection.base_url, &selection)
}

fn launch_claude(
    args: &HarnessArgs,
    target: &std::path::Path,
    base_url: &str,
    selection: &models::ClaudeModelSelection,
) -> Result<(), TcampError> {
    let binary = harness::launch::require_binary(Harness::Claude)?;
    let api_key = credentials::require_launch_key(base_url)?;
    let argv = claude::launch_args(target, &args.harness_args, selection);
    let environment = claude::launch_environment(&api_key, selection);

    eprintln!("{}", claude::MESSAGES_UNVERIFIED_HINT);
    eprintln!("Launching claude ...");
    harness::launch::exec(Harness::Claude, &binary, &argv, &environment)
}

fn configure_pi(args: HarnessArgs, json: bool) -> Result<(), TcampError> {
    let mut connection = credentials::resolve_connection(args.base_url.as_deref())?;
    let selection = pi::resolve_model(args.model.as_deref())?;
    if !args.no_launch && !args.dry_run {
        connection = credentials::ensure_launch_key(connection, terminal::is_interactive())?;
    }
    let connection = connection.require_key()?;
    let paths = pi::paths()?;
    let model_ids = vec![selection.model.clone()];

    let mut warnings = connection.warnings.clone();
    if !args.dry_run && !args.no_launch {
        let _ = harness::launch::require_binary(Harness::Pi)?;
    }

    let shadowing = pi::inspect_shadowing()?;
    if args.dry_run || args.no_launch {
        warnings.extend(shadowing.warnings());
    } else if !shadowing.is_empty() {
        return Err(shadowing.error());
    }

    let settings_target = paths.settings.clone();
    let settings_existing = config::read_target(&settings_target)?;
    let settings_merge =
        pi::merge_settings(settings_existing.contents.as_deref(), &settings_target)?;
    let settings_written = settings_merge.contents.is_some();

    if !args.dry_run {
        let existing = config::read_target(&paths.models)?;
        let manifest = config::manifest::Manifest::load()?;
        let key = existing.resolved.path.display().to_string();
        pi::inspect_models(
            existing.contents.as_deref(),
            &paths.models,
            manifest.targets.get(&key),
            args.force,
        )?;

        let api_key = credentials::require_launch_key(&connection.base_url)?;
        let model_check =
            harness::preflight::check_models(&connection.base_url, &api_key, &model_ids);
        if !model_check.missing.is_empty() {
            return Err(TcampError::usage(
                format!(
                    "the model(s) {} were not found at {}",
                    model_check.missing.join(", "),
                    connection.base_url
                ),
                "pass a different model with `--model`, or upgrade tcamp when the curated catalog changes",
            ));
        }
        warnings.extend(model_check.warnings);
    }

    let models_target = paths.models.clone();
    let base_url = connection.base_url.clone();
    let desired = move |existing: &config::TargetContents| {
        pi::merge_models(existing.contents.as_deref(), &models_target, &base_url)
    };

    let inspect_target = paths.models.clone();
    let force = args.force;
    let inspect = move |existing: &config::TargetContents,
                        entry: Option<&config::manifest::ManifestEntry>|
          -> Result<config::engine::Inspection, TcampError> {
        pi::inspect_models(existing.contents.as_deref(), &inspect_target, entry, force)
    };

    let prompt = !json && !args.yes && terminal::is_tty();
    let outcome = config::engine::converge(config::engine::Request {
        harness: pi::HARNESS_ID,
        target: paths.models.clone(),
        desired: &desired,
        dry_run: args.dry_run,
        prompt,
        inspect: &inspect,
    })?;
    warnings.extend(outcome.warnings);

    if outcome.declined {
        if json {
            return output::print_json(&SuccessReport {
                ok: true,
                base_url: Some(connection.base_url),
                key: key_report(
                    connection.masked_key,
                    connection.source,
                    connection.verified_at,
                ),
                harnesses: harness_report(pi::HARNESS_ID, &paths.models, "none", true),
                warnings: warning_reports(&warnings),
            });
        }
        print_warnings(&warnings);
        println!("No changes.");
        return Ok(());
    }

    let settings_outcome = if settings_written {
        let settings_target = paths.settings.clone();
        let desired = move |existing: &config::TargetContents| {
            let unchanged = existing.contents.clone().unwrap_or_default();
            let merge = pi::merge_settings(existing.contents.as_deref(), &settings_target)?;
            Ok(merge.contents.unwrap_or(unchanged))
        };
        let settings_target = paths.settings.clone();
        let inspect = move |existing: &config::TargetContents,
                            _entry: Option<&config::manifest::ManifestEntry>|
              -> Result<config::engine::Inspection, TcampError> {
            pi::inspect_settings(existing.contents.as_deref(), &settings_target)
        };
        let settings_outcome = config::engine::converge(config::engine::Request {
            harness: pi::HARNESS_ID,
            target: paths.settings.clone(),
            desired: &desired,
            dry_run: args.dry_run,
            prompt: false,
            inspect: &inspect,
        })?;
        warnings.extend(settings_outcome.warnings.clone());
        Some(settings_outcome)
    } else {
        None
    };

    let settings_changed = settings_outcome
        .as_ref()
        .is_some_and(|outcome| outcome.action != config::engine::Action::None);
    let any_change = outcome.action != config::engine::Action::None || settings_changed;
    let primary_action = if outcome.action != config::engine::Action::None {
        outcome.action
    } else if settings_changed {
        settings_outcome.as_ref().expect("settings changed").action
    } else {
        config::engine::Action::None
    };

    if args.dry_run {
        let mut changes = vec![ChangeReport {
            path: paths.models.display().to_string(),
            action: outcome.action.as_str(),
            diff: outcome.diff.unwrap_or_default(),
        }];
        if let Some(settings) = &settings_outcome {
            if settings.action != config::engine::Action::None {
                changes.push(ChangeReport {
                    path: paths.settings.display().to_string(),
                    action: settings.action.as_str(),
                    diff: settings.diff.clone().unwrap_or_default(),
                });
            }
        }
        return print_dry_run(
            connection,
            pi::HARNESS_ID,
            paths.models.clone(),
            primary_action.as_str(),
            changes,
            warnings,
            json,
        );
    }

    let harnesses = harness_report(pi::HARNESS_ID, &paths.models, primary_action.as_str(), true);

    if json {
        return output::print_json(&SuccessReport {
            ok: true,
            base_url: Some(connection.base_url),
            key: key_report(
                connection.masked_key,
                connection.source,
                connection.verified_at,
            ),
            harnesses,
            warnings: warning_reports(&warnings),
        });
    }

    if args.no_launch {
        print_warnings(&warnings);
        if any_change {
            println!("Configured pi: {}", paths.models.display());
        } else {
            println!("No changes.");
        }
        return Ok(());
    }

    let binary = harness::launch::require_binary(Harness::Pi)?;
    if let Some(warning) = harness::launch::version_warning(Harness::Pi, &binary, pi::MIN_VERSION) {
        warnings.push(warning);
    }
    print_warnings(&warnings);

    if any_change {
        eprintln!("Configured pi: {}", paths.models.display());
    }

    let api_key = credentials::require_launch_key(&connection.base_url)?;
    let argv = pi::launch_args(&selection, &args.harness_args);
    let environment = pi::launch_environment(&api_key);
    eprintln!("Launching pi ...");
    harness::launch::exec(Harness::Pi, &binary, &argv, &environment)
}

fn configure_kimi(args: HarnessArgs, json: bool) -> Result<(), TcampError> {
    let mut connection = credentials::resolve_connection(args.base_url.as_deref())?;
    let selection = kimi::resolve_model(args.model.as_deref())?;
    if !args.no_launch && !args.dry_run {
        connection = credentials::ensure_launch_key(connection, terminal::is_interactive())?;
    }
    let connection = connection.require_key()?;
    let paths = kimi::paths()?;
    let model_ids = vec![selection.model.clone()];

    let mut warnings = connection.warnings.clone();
    if !args.dry_run && !args.no_launch {
        let _ = harness::launch::require_binary(Harness::Kimi)?;
    }

    if !args.dry_run {
        let existing = config::read_target(&paths.config)?;
        let manifest = config::manifest::Manifest::load()?;
        let key = existing.resolved.path.display().to_string();
        kimi::inspect_config(
            existing.contents.as_deref(),
            &paths.config,
            manifest.targets.get(&key),
            args.force,
        )?;

        let api_key = credentials::require_launch_key(&connection.base_url)?;
        let model_check =
            harness::preflight::check_models(&connection.base_url, &api_key, &model_ids);
        if !model_check.missing.is_empty() {
            return Err(TcampError::usage(
                format!(
                    "the model(s) {} were not found at {}",
                    model_check.missing.join(", "),
                    connection.base_url
                ),
                "pass a different model with `--model`, or upgrade tcamp when the curated catalog changes",
            ));
        }
        warnings.extend(model_check.warnings);
    }

    let target = paths.config.clone();
    let base_url = connection.base_url.clone();
    let desired = move |existing: &config::TargetContents| {
        kimi::merge_config(existing.contents.as_deref(), &target, &base_url)
    };

    let inspect_target = paths.config.clone();
    let force = args.force;
    let inspect = move |existing: &config::TargetContents,
                        entry: Option<&config::manifest::ManifestEntry>|
          -> Result<config::engine::Inspection, TcampError> {
        kimi::inspect_config(existing.contents.as_deref(), &inspect_target, entry, force)
    };

    let prompt = !json && !args.yes && terminal::is_tty();
    let outcome = config::engine::converge(config::engine::Request {
        harness: kimi::HARNESS_ID,
        target: paths.config.clone(),
        desired: &desired,
        dry_run: args.dry_run,
        prompt,
        inspect: &inspect,
    })?;
    warnings.extend(outcome.warnings);

    if outcome.declined {
        if json {
            return output::print_json(&SuccessReport {
                ok: true,
                base_url: Some(connection.base_url),
                key: key_report(
                    connection.masked_key,
                    connection.source,
                    connection.verified_at,
                ),
                harnesses: harness_report(kimi::HARNESS_ID, &paths.config, "none", true),
                warnings: warning_reports(&warnings),
            });
        }
        print_warnings(&warnings);
        println!("No changes.");
        return Ok(());
    }

    if args.dry_run {
        let action = outcome.action.as_str();
        return print_dry_run(
            connection,
            kimi::HARNESS_ID,
            paths.config.clone(),
            action,
            vec![ChangeReport {
                path: paths.config.display().to_string(),
                action,
                diff: outcome.diff.unwrap_or_default(),
            }],
            warnings,
            json,
        );
    }

    let harnesses = harness_report(
        kimi::HARNESS_ID,
        &paths.config,
        outcome.action.as_str(),
        true,
    );

    if json {
        return output::print_json(&SuccessReport {
            ok: true,
            base_url: Some(connection.base_url),
            key: key_report(
                connection.masked_key,
                connection.source,
                connection.verified_at,
            ),
            harnesses,
            warnings: warning_reports(&warnings),
        });
    }

    if args.no_launch {
        print_warnings(&warnings);
        if outcome.action == config::engine::Action::None {
            println!("No changes.");
        } else {
            println!("Configured kimi: {}", paths.config.display());
        }
        return Ok(());
    }

    let binary = harness::launch::require_binary(Harness::Kimi)?;
    if let Some(warning) =
        harness::launch::version_warning(Harness::Kimi, &binary, kimi::MIN_VERSION)
    {
        warnings.push(warning);
    }
    print_warnings(&warnings);

    if outcome.action != config::engine::Action::None {
        eprintln!("Configured kimi: {}", paths.config.display());
    }

    let api_key = credentials::require_launch_key(&connection.base_url)?;
    let argv = kimi::launch_args(&selection, &args.harness_args);
    let environment = kimi::launch_environment(&api_key);
    eprintln!("Launching kimi ...");
    harness::launch::exec(Harness::Kimi, &binary, &argv, &environment)
}

fn print_login_report(report: LoginReport, json: bool) -> Result<(), TcampError> {
    if json {
        return output::print_json(&SuccessReport {
            ok: true,
            base_url: Some(report.base_url),
            key: Some(KeyReport {
                masked: report.masked_key,
                source: report.source.as_str(),
                verified: report.verified_at.is_some(),
                verified_at: report.verified_at,
            }),
            harnesses: BTreeMap::new(),
            warnings: warning_reports(&report.warnings),
        });
    }

    if report.source == KeySource::Env {
        eprintln!("Using API key from TOKENCAMP_API_KEY");
    }
    println!("Saved API key: {}", report.masked_key);
    println!("Base URL: {}", report.base_url);
    if let Some(verified_at) = &report.verified_at {
        println!("Verified at: {verified_at}");
    }
    print_warnings(&report.warnings);
    Ok(())
}

fn print_logout_report(report: LogoutReport, json: bool) -> Result<(), TcampError> {
    if json {
        return output::print_json(&SuccessReport {
            ok: true,
            base_url: report.base_url,
            key: None,
            harnesses: BTreeMap::new(),
            warnings: warning_reports(&report.warnings),
        });
    }

    if report.all {
        if report.removed.is_empty() {
            println!("No stored credentials.");
        } else {
            println!("Removed all stored credentials.");
        }
    } else if let Some(base_url) = &report.base_url {
        if report.removed.contains(base_url) {
            println!("Removed credential for {base_url}.");
        } else {
            println!("No credential found for {base_url}.");
        }
    } else {
        println!("No active connection.");
    }
    print_warnings(&report.warnings);
    Ok(())
}

#[derive(Serialize)]
struct StatusReport {
    ok: bool,
    base_url: String,
    key: Option<KeyReport>,
    credentials: String,
    harnesses: BTreeMap<String, HarnessStatusReport>,
    warnings: Vec<WarningReport>,
}

#[derive(Serialize)]
struct HarnessStatusReport {
    installed: bool,
    configured: bool,
    target: String,
    conflict: bool,
}

fn print_status_report(report: status::Snapshot, json: bool) -> Result<(), TcampError> {
    let key = key_report(
        report.connection.masked_key.clone(),
        report.connection.source,
        report.connection.verified_at.clone(),
    );
    let harnesses: BTreeMap<String, HarnessStatusReport> = report
        .harnesses
        .iter()
        .map(|(name, status)| {
            (
                (*name).to_string(),
                HarnessStatusReport {
                    installed: status.installed,
                    configured: status.configured,
                    target: status.target.display().to_string(),
                    conflict: status.conflict,
                },
            )
        })
        .collect();

    if json {
        return output::print_json(&StatusReport {
            ok: true,
            base_url: report.connection.base_url,
            key,
            credentials: report.connection.credentials_path.display().to_string(),
            harnesses,
            warnings: warning_reports(&report.warnings),
        });
    }

    println!("Base URL: {}", report.connection.base_url);
    match &key {
        Some(key) => println!("API key: {} (source: {})", key.masked, key.source),
        None => println!("API key: none"),
    }
    if let Some(verified_at) = &report.connection.verified_at {
        println!("Verified at: {verified_at}");
    }
    println!(
        "Credentials: {}",
        report.connection.credentials_path.display()
    );
    println!();
    println!("Harness  Installed  Configured  Conflict  Target");
    for (name, status) in &report.harnesses {
        println!(
            "{:<8} {:<10} {:<11} {:<9} {}",
            name,
            yes_no(status.installed),
            yes_no(status.configured),
            yes_no(status.conflict),
            status.target.display()
        );
    }
    print_warnings(&report.warnings);
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn print_warnings(warnings: &[Warning]) {
    for warning in warnings {
        eprintln!("warning: {} [{}]", warning.message, warning.code);
    }
}
