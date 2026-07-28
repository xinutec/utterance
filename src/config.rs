//! Runtime configuration, read from the environment at startup.

use std::path::PathBuf;

/// Defaults, named once so the help text and the code cannot disagree.
///
/// A usage message that lists a default the program does not use is worse than
/// no usage message: it is believed.
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8181";
const DEFAULT_DATA_DIR: &str = "data";

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

/// Read the command line, which until now was ignored entirely.
///
/// **Why a binary that took no arguments needed this.** It never did while it
/// was launched from a script that already knew how to configure it. Installed
/// as a package and run by name, `utterance --help` is the only way anyone finds out
/// that it is configured by three environment variables at all — and what it did
/// instead was start a server, which looks like a hang, and then fail to bind
/// because one was already running.
///
/// An unrecognised argument is an error rather than something to ignore. Quietly
/// ignoring arguments is exactly how `--help` came to launch a server: a typo,
/// or a flag this program does not have, should say so rather than do something
/// else confidently.
///
/// **Every argument is read, not just the first.** The obvious shape — a loop
/// that matches and returns — inspects one and silently drops the rest, which is
/// the same failure again a step further along: `utterance --version --sereve` would
/// print a version and never mention the typo. Clippy caught that one; it is
/// written down here because the shape is easy to reach for again.
pub fn invocation<I: IntoIterator<Item = String>>(args: I) -> Result<Invocation, String> {
    let args: Vec<String> = args.into_iter().collect();
    let is = |arg: &String, short: &str, long: &str| arg == short || arg == long;

    // Complained about before anything is honoured, so a command line that is
    // part sense and part nonsense is refused rather than half-obeyed.
    if let Some(unknown) = args
        .iter()
        .find(|a| !is(a, "-h", "--help") && !is(a, "-V", "--version"))
    {
        return Err(format!("unrecognised argument {unknown}\n\n{}", usage()));
    }

    if args.iter().any(|a| is(a, "-h", "--help")) {
        return Ok(Invocation::Print(usage()));
    }
    if args.iter().any(|a| is(a, "-V", "--version")) {
        return Ok(Invocation::Print(format!(
            "{} {}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        )));
    }
    Ok(Invocation::Serve)
}

/// What the program accepts, in the form someone reads when they are stuck.
///
/// Every setting is an environment variable, so the help is mostly a list of
/// them. That is unusual enough to be worth saying out loud rather than leaving
/// someone to conclude the program is unconfigurable.
fn usage() -> String {
    format!(
        "\
{name} {version} — derive music from the structure of a voice.

Usage: {name} [--help] [--version]

Runs an HTTP server. There are no options: everything is configured by the
environment, so that one launcher can set it and every way of starting the
program agrees.

Environment:
  BIND_ADDR   (default {bind})
              address to listen on
  DATA_DIR    (default {data})
              where recordings and their voiceprints are kept
  STATIC_DIR  (default unset)
              built Angular bundle to serve. Unset serves the API alone,
              which is what `ng serve` expects
  RUST_LOG    (default info)
              tracing filter
",
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
        bind = DEFAULT_BIND_ADDR,
        data = DEFAULT_DATA_DIR,
    )
}
