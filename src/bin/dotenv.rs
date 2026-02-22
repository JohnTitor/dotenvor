use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{self, Command};

use dotenvor::{EnvLoader, Error, KeyParsingMode, SubstitutionMode, TargetEnv};

const DEFAULT_FILE: &str = ".env";

const HELP: &str = "\
dotenv - run commands with variables loaded from dotenv files

Usage:
  dotenv run [OPTIONS] -- COMMAND [ARGS...]
  dotenv run [OPTIONS] COMMAND [ARGS...]
  dotenv --help
  dotenv --version

Commands:
  run       Load dotenv files and execute a command
";

const RUN_HELP: &str = "\
dotenv run - load dotenv files and execute a command

Usage:
  dotenv run [OPTIONS] -- COMMAND [ARGS...]
  dotenv run [OPTIONS] COMMAND [ARGS...]

Options:
  -f, --file <PATHS>      Dotenv file path(s). Repeat or pass comma-separated paths.
                          Defaults to .env.
  -i, --ignore            Ignore missing dotenv files.
      --ignore-missing    Alias for --ignore.
  -o, --override          Override existing environment variables.
      --overload          Alias for --override.
  -u, --search-upward     Search parent directories for relative dotenv files.
      --expand            Expand variable placeholders in values.
      --permissive-keys   Accept permissive key syntax.
  -v, --verbose           Print loader diagnostics to stderr.
  -q, --quiet             Suppress loader diagnostics.
  -h, --help              Show this help text.
";

#[derive(Debug, Clone, PartialEq, Eq)]
enum RunCommand {
    Help,
    Execute(RunOptions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunOptions {
    files: Vec<PathBuf>,
    required: bool,
    override_existing: bool,
    search_upward: bool,
    substitution_mode: SubstitutionMode,
    key_parsing_mode: KeyParsingMode,
    verbose: bool,
    quiet: bool,
    command: OsString,
    args: Vec<OsString>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            required: true,
            override_existing: false,
            search_upward: false,
            substitution_mode: SubstitutionMode::Disabled,
            key_parsing_mode: KeyParsingMode::Strict,
            verbose: false,
            quiet: false,
            command: OsString::new(),
            args: Vec::new(),
        }
    }
}

fn main() {
    process::exit(run(env::args_os()));
}

fn run(args: impl IntoIterator<Item = OsString>) -> i32 {
    let mut args = args.into_iter();
    let _bin = args.next();

    let Some(subcommand) = args.next() else {
        print_help();
        return 0;
    };

    let subcommand = subcommand.to_string_lossy();
    match subcommand.as_ref() {
        "-h" | "--help" | "help" => {
            print_help();
            0
        }
        "-V" | "--version" | "version" => {
            print_version();
            0
        }
        "run" => match parse_run_options(args.collect()) {
            Ok(RunCommand::Help) => {
                print_run_help();
                0
            }
            Ok(RunCommand::Execute(options)) => match execute_run(options) {
                Ok(code) => code,
                Err(err) => {
                    eprintln!("dotenv: {err}");
                    1
                }
            },
            Err(err) => {
                eprintln!("dotenv: {err}");
                eprintln!("Try `dotenv run --help`.");
                1
            }
        },
        unknown => {
            eprintln!("dotenv: unknown subcommand `{unknown}`");
            eprintln!("Try `dotenv --help`.");
            1
        }
    }
}

fn parse_run_options(args: Vec<OsString>) -> Result<RunCommand, String> {
    let mut options = RunOptions::default();
    let mut index = 0usize;
    while index < args.len() {
        let token = args[index].to_string_lossy();
        match token.as_ref() {
            "--" => {
                index += 1;
                break;
            }
            "-h" | "--help" => return Ok(RunCommand::Help),
            "-f" | "--file" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value for `-f/--file`".to_owned());
                };
                parse_file_values(value, &mut options.files)?;
                index += 1;
            }
            value if value.starts_with("--file=") => {
                parse_file_text(&value["--file=".len()..], &mut options.files)?;
                index += 1;
            }
            "-i" | "--ignore" | "--ignore-missing" => {
                options.required = false;
                index += 1;
            }
            "-o" | "--override" | "--overload" => {
                options.override_existing = true;
                index += 1;
            }
            "-u" | "--search-upward" => {
                options.search_upward = true;
                index += 1;
            }
            "--expand" => {
                options.substitution_mode = SubstitutionMode::Expand;
                index += 1;
            }
            "--permissive-keys" => {
                options.key_parsing_mode = KeyParsingMode::Permissive;
                index += 1;
            }
            "-v" | "--verbose" => {
                options.verbose = true;
                index += 1;
            }
            "-q" | "--quiet" => {
                options.quiet = true;
                index += 1;
            }
            unknown if unknown.starts_with('-') => {
                return Err(format!("unknown option `{unknown}`"));
            }
            _ => break,
        }
    }

    let remaining = &args[index..];
    let Some((command, command_args)) = remaining.split_first() else {
        return Err("missing command after `run`".to_owned());
    };

    if options.files.is_empty() {
        options.files.push(PathBuf::from(DEFAULT_FILE));
    }

    options.command = command.clone();
    options.args = command_args.to_vec();
    Ok(RunCommand::Execute(options))
}

fn parse_file_values(raw: &OsString, files: &mut Vec<PathBuf>) -> Result<(), String> {
    parse_file_text(&raw.to_string_lossy(), files)
}

fn parse_file_text(raw: &str, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut added = 0usize;
    for segment in raw.split(',') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        files.push(PathBuf::from(trimmed));
        added += 1;
    }
    if added == 0 {
        return Err("`-f/--file` requires at least one path".to_owned());
    }
    Ok(())
}

fn execute_run(options: RunOptions) -> Result<i32, String> {
    let entries = load_entries(&options).map_err(format_loader_error)?;
    let mut command = Command::new(&options.command);
    command.args(&options.args);

    for entry in entries {
        if !options.override_existing && env::var_os(&entry.key).is_some() {
            continue;
        }
        command.env(entry.key, entry.value);
    }

    execute_command(command, &options.command)
}

fn load_entries(options: &RunOptions) -> Result<Vec<dotenvor::Entry>, Error> {
    let env_snapshot = snapshot_process_env();
    let loader = EnvLoader::new()
        .paths(&options.files)
        .required(options.required)
        .override_existing(options.override_existing)
        .search_upward(options.search_upward)
        .substitution_mode(options.substitution_mode)
        .key_parsing_mode(options.key_parsing_mode)
        .verbose(options.verbose)
        .quiet(options.quiet)
        .target(TargetEnv::from_memory(env_snapshot));
    loader.parse_only()
}

fn snapshot_process_env() -> BTreeMap<String, String> {
    env::vars_os()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

#[cfg(unix)]
fn execute_command(mut command: Command, program: &OsString) -> Result<i32, String> {
    let err = command.exec();
    Err(format!(
        "failed to execute `{}`: {err}",
        program.to_string_lossy()
    ))
}

#[cfg(not(unix))]
fn execute_command(mut command: Command, program: &OsString) -> Result<i32, String> {
    let status = command
        .status()
        .map_err(|err| format!("failed to execute `{}`: {err}", program.to_string_lossy()))?;
    Ok(status.code().unwrap_or(1))
}

fn format_loader_error(err: Error) -> String {
    match err {
        Error::Io(io_err) => format!("I/O error: {io_err}"),
        Error::Parse(parse_err) => parse_err.to_string(),
        Error::InvalidEncoding(utf8_err) => format!("invalid UTF-8 input: {utf8_err}"),
    }
}

fn print_help() {
    println!("{HELP}");
}

fn print_run_help() {
    println!("{RUN_HELP}");
}

fn print_version() {
    println!("dotenv {}", env!("CARGO_PKG_VERSION"));
}

#[cfg(test)]
mod tests {
    use super::{RunCommand, RunOptions, parse_run_options};
    use dotenvor::{KeyParsingMode, SubstitutionMode};
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[test]
    fn parse_run_uses_defaults() {
        let parsed = parse_run_options(vec![OsString::from("printenv"), OsString::from("FOO")])
            .expect("parse should succeed");
        let RunCommand::Execute(options) = parsed else {
            panic!("expected execute");
        };

        assert_eq!(options.files, vec![PathBuf::from(".env")]);
        assert!(options.required);
        assert!(!options.override_existing);
        assert!(!options.search_upward);
        assert_eq!(options.substitution_mode, SubstitutionMode::Disabled);
        assert_eq!(options.key_parsing_mode, KeyParsingMode::Strict);
        assert_eq!(options.command, OsString::from("printenv"));
        assert_eq!(options.args, vec![OsString::from("FOO")]);
    }

    #[test]
    fn parse_run_supports_repeated_and_comma_separated_files() {
        let parsed = parse_run_options(vec![
            OsString::from("-f"),
            OsString::from(".env.local,.env"),
            OsString::from("--file"),
            OsString::from("custom.env"),
            OsString::from("--"),
            OsString::from("printenv"),
            OsString::from("FOO"),
        ])
        .expect("parse should succeed");
        let RunCommand::Execute(options) = parsed else {
            panic!("expected execute");
        };

        assert_eq!(
            options.files,
            vec![
                PathBuf::from(".env.local"),
                PathBuf::from(".env"),
                PathBuf::from("custom.env"),
            ]
        );
    }

    #[test]
    fn parse_run_reports_missing_file_value() {
        let err = parse_run_options(vec![OsString::from("-f")]).expect_err("parse should fail");
        assert_eq!(err, "missing value for `-f/--file`");
    }

    #[test]
    fn parse_run_rejects_empty_file_list() {
        let err = parse_run_options(vec![
            OsString::from("-f"),
            OsString::from(","),
            OsString::from("printenv"),
            OsString::from("FOO"),
        ])
        .expect_err("parse should fail");
        assert_eq!(err, "`-f/--file` requires at least one path");
    }

    #[test]
    fn parse_run_help_short_circuits() {
        let parsed = parse_run_options(vec![OsString::from("--help")]).expect("parse should work");
        assert_eq!(parsed, RunCommand::Help);
    }

    #[test]
    fn run_options_default_matches_expected_behavior() {
        let options = RunOptions::default();
        assert!(options.required);
        assert!(!options.override_existing);
        assert!(!options.search_upward);
    }
}
