use std::path::{Path, PathBuf};

use aideon_tools::aideon::tools::io::rdf::{JsonLdProfileSet, RdfFormat};
use aideon_tools::aideon::tools::sync;
use aideon_tools::{Result, ToolError};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::Value;

fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Sync(args) => execute_sync(args),
    }
}

fn execute_sync(args: SyncArgs) -> Result<()> {
    if !args.input.exists() {
        return Err(ToolError::MissingInput(args.input));
    }

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

fn guess_rdf_format(path: &Path) -> RdfFormat {
    aideon_tools::io::rdf::detect_format(path).unwrap_or(RdfFormat::Turtle)
}

fn load_json(path: &PathBuf) -> Result<Value> {
    let data = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Round-trip JSON-LD, RDF, and Excel data sets."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Synchronise two representations of the dataset.
    Sync(SyncArgs),
}

#[derive(clap::Args)]
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
