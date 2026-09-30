//! Runtime configuration, read from the environment at startup.

use std::path::PathBuf;

use clap::{CommandFactory, Parser};

/// Defaults, named once so the help text and the code cannot disagree.
///
/// A usage message that lists a default the program does not use is worse than
/// no usage message: it is believed.
///
/// Public so `tests/cli.rs` can assert the help quotes these without mutating
/// process-global environment under parallel tests — the race edition 2024 made
/// `remove_var` unsafe for.
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8181";
pub const DEFAULT_DATA_DIR: &str = "data";

#[derive(Clone, Debug)]
pub struct Config {
    /// Address to bind the HTTP server to.
    pub bind_addr: String,
    /// Where recordings and their voiceprints live.
    pub data_dir: PathBuf,
    /// Directory of the built Angular bundle to serve, with SPA fallback. Unset
    /// in dev, where `ng serve` proxies `/api` here and serves the app itself.
    pub static_dir: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string()),
            data_dir: std::env::var("DATA_DIR")
                .map_or_else(|_| PathBuf::from(DEFAULT_DATA_DIR), PathBuf::from),
            static_dir: std::env::var("STATIC_DIR").ok().map(PathBuf::from),
        }
    }
}

/// What the command line asked the program to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invocation {
    /// Start the server.
    Serve,
    /// Write this to stdout and exit successfully.
    Print(String),
}

/// The command line. Help and version are ordinary flags rather than clap's,
/// which answer the moment they are seen: `--help --sereve` must report the
/// typo, since every argument is checked before any is honoured.
#[derive(Parser)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    about = "Derive music from the structure of a voice. Runs an HTTP server.",
    disable_help_flag = true,
    disable_version_flag = true,
    after_help = environment()
)]
struct Cli {
    /// Print help.
    #[arg(short, long)]
    help: bool,
    /// Print the version.
    #[arg(short = 'V', long)]
    version: bool,
}

/// Read the process's command line; see [`invocation`].
pub fn invocation_from_env() -> Result<Invocation, String> {
    answer(Cli::try_parse())
}

/// Read a command line. Installed and run by name, `utterance --help` is how
/// anyone learns it is configured by environment; an unknown argument is an
/// error, and every argument is checked, so `--version --sereve` reports the
/// typo.
pub fn invocation<I: IntoIterator<Item = String>>(args: I) -> Result<Invocation, String> {
    let name = env!("CARGO_PKG_NAME").to_string();
    answer(Cli::try_parse_from(std::iter::once(name).chain(args)))
}

fn answer(parsed: Result<Cli, clap::Error>) -> Result<Invocation, String> {
    let cli =
        parsed.map_err(|e| format!("{}\nTry `{} --help`.", e.render(), env!("CARGO_PKG_NAME")))?;
    if cli.help {
        return Ok(Invocation::Print(Cli::command().render_help().to_string()));
    }
    if cli.version {
        return Ok(Invocation::Print(format!(
            "{} {}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        )));
    }
    Ok(Invocation::Serve)
}

/// There are no options: everything is configured by the environment, so that
/// one launcher can set it and every way of starting the program agrees. That
/// is unusual enough to say out loud rather than leave someone to conclude the
/// program is unconfigurable.
fn environment() -> String {
    format!(
        "\
Environment:
  BIND_ADDR   (default {DEFAULT_BIND_ADDR})
              address to listen on
  DATA_DIR    (default {DEFAULT_DATA_DIR})
              where recordings and their voiceprints are kept
  STATIC_DIR  (default unset)
              built Angular bundle to serve. Unset serves the API alone,
              which is what `ng serve` expects
  RUST_LOG    (default info)
              tracing filter"
    )
}
