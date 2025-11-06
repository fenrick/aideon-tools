//! Command-line entrypoint for the aideon-tools binary.
//!
//! The CLI orchestrates the conversion routines exposed by the library while
//! providing structured logging that can be tuned per invocation.

use std::path::{Path, PathBuf};

use aideon_tools::aideon::tools::io::rdf::{JsonLdProfileSet, RdfFormat};
use aideon_tools::aideon::tools::sync;
use aideon_tools::{Result, ToolError};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::Value;
use tracing::{debug, error};
use tracing_subscriber::EnvFilter;

fn main() {
    let cli = Cli::parse();

    if let Err(error) = init_tracing(cli.log_level) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }

    if let Err(error) = run(cli) {
        error!(%error, "CLI execution failed");
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

/// Executes the CLI command selected by the user.
fn run(cli: Cli) -> Result<()> {
    debug!(command = ?cli.command, "dispatching command");
    match cli.command {
        Command::Sync(args) => execute_sync(args),
    }
}

/// Executes the sync subcommand by delegating to the appropriate conversion
/// routine.
fn execute_sync(args: SyncArgs) -> Result<()> {
    if !args.input.exists() {
        return Err(ToolError::MissingInput(args.input));
    }

    debug!(
        from = %args.from,
        to = %args.to,
        input = %args.input.display(),
        output = %args.output.display(),
        has_context = args.context.is_some(),
        "resolved sync arguments"
    );

    let context = match &args.context {
        Some(path) => Some(load_json(path)?),
        None => None,
    };

    match (args.from, args.to) {
        (DataFormat::JsonLd, DataFormat::Excel) => sync::jsonld_to_excel(&args.input, &args.output),
        (DataFormat::Excel, DataFormat::JsonLd) => {
            sync::excel_to_jsonld(&args.input, &args.output, context)
        }
        (DataFormat::JsonLd, DataFormat::Rdf) => {
            let format = args.resolve_rdf_format(&args.output);
            sync::jsonld_to_rdf(&args.input, &args.output, format)
        }
        (DataFormat::Excel, DataFormat::Rdf) => {
            let format = args.resolve_rdf_format(&args.output);
            sync::excel_to_rdf(&args.input, &args.output, format)
        }
        (DataFormat::Rdf, DataFormat::Excel) => sync::rdf_to_excel(&args.input, &args.output),
        (DataFormat::Rdf, DataFormat::JsonLd) => {
            sync::rdf_to_jsonld(&args.input, &args.output, context)
        }
        _ => Err(ToolError::UnsupportedConversion {
            from: args.from.to_string(),
            to: args.to.to_string(),
        }),
    }
}

/// Attempts to infer the RDF serialisation from a target path when none was
/// provided explicitly.
fn guess_rdf_format(path: &Path) -> RdfFormat {
    aideon_tools::io::rdf::detect_format(path).unwrap_or(RdfFormat::Turtle)
}

/// Loads a JSON value from the supplied path.
fn load_json(path: &PathBuf) -> Result<Value> {
    let data = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

/// Command-line interface definition for the aideon tools.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Round-trip JSON-LD, RDF, and Excel data sets."
)]
struct Cli {
    /// Desired minimum log level for the current invocation.
    #[arg(long, value_enum, default_value_t = LogLevel::Info, global = true)]
    log_level: LogLevel,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Synchronise two representations of the dataset.
    Sync(SyncArgs),
}

#[derive(clap::Args, Debug)]
struct SyncArgs {
    /// Source representation.
    #[arg(long, value_enum)]
    from: DataFormat,

    /// Input file path.
    #[arg(long)]
    input: PathBuf,

    /// Target representation.
    #[arg(long, value_enum)]
    to: DataFormat,

    /// Output file path.
    #[arg(long)]
    output: PathBuf,

    /// Optional JSON-LD context to use when serialising.
    #[arg(long)]
    context: Option<PathBuf>,

    /// Explicit RDF serialisation format to use when writing RDF files.
    #[arg(long, value_enum)]
    rdf_format: Option<RdfFormatKind>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum DataFormat {
    JsonLd,
    Excel,
    Rdf,
}

impl std::fmt::Display for DataFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataFormat::JsonLd => write!(f, "jsonld"),
            DataFormat::Excel => write!(f, "xlsx"),
            DataFormat::Rdf => write!(f, "rdf"),
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum RdfFormatKind {
    Turtle,
    NTriples,
    NQuads,
    TriG,
    JsonLd,
}

impl From<RdfFormatKind> for RdfFormat {
    fn from(kind: RdfFormatKind) -> Self {
        match kind {
            RdfFormatKind::Turtle => RdfFormat::Turtle,
            RdfFormatKind::NTriples => RdfFormat::NTriples,
            RdfFormatKind::NQuads => RdfFormat::NQuads,
            RdfFormatKind::TriG => RdfFormat::TriG,
            RdfFormatKind::JsonLd => RdfFormat::JsonLd {
                profile: JsonLdProfileSet::empty(),
            },
        }
    }
}

impl SyncArgs {
    fn resolve_rdf_format(&self, output: &Path) -> RdfFormat {
        self.rdf_format
            .map(RdfFormat::from)
            .unwrap_or_else(|| guess_rdf_format(output))
    }
}

/// Supported logging levels exposed as CLI values.
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    fn as_directive(self) -> tracing_subscriber::filter::Directive {
        use tracing::Level;

        let level = match self {
            LogLevel::Error => Level::ERROR,
            LogLevel::Warn => Level::WARN,
            LogLevel::Info => Level::INFO,
            LogLevel::Debug => Level::DEBUG,
            LogLevel::Trace => Level::TRACE,
        };

        level.into()
    }
}

/// Configures the global tracing subscriber based on the selected log level or
/// the `RUST_LOG` environment variable.
fn init_tracing(level: LogLevel) -> Result<()> {
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::default().add_directive(level.as_directive()),
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|error| ToolError::Logging(error.to_string()))
}
