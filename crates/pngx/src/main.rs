mod commands;
mod config;
mod output;
mod resolve;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{ArgAction, Parser, Subcommand};
use pngx_client::ApiError;
use tracing_subscriber::EnvFilter;

use config::RawConfig;
use output::OutputFormat;

#[derive(Parser)]
#[command(
    name = "pngx",
    about = "CLI for Paperless NGX",
    long_about = "CLI for Paperless-ngx, the community-maintained document management system that \
        transforms physical documents into a searchable online archive.",
    after_long_help = "GETTING STARTED:\n  \
        pngx auth login              Save server URL and API token\n  \
        pngx auth status             Verify connection\n\n\
        COMMON WORKFLOWS:\n  \
        pngx search \"invoice 2024\"   Find documents matching a query\n  \
        pngx documents get 42 43     View document details\n  \
        pngx documents content 42    Read document text\n  \
        pngx documents open 42 43    Open in the web UI\n  \
        pngx tags                    List all tags\n\n\
        OUTPUT:\n  \
        Default output is markdown tables. Use -o json for structured output.",
    version
)]
struct Cli {
    /// Paperless NGX server URL
    #[arg(long, global = true)]
    url: Option<String>,

    /// API authentication token
    #[arg(long, global = true)]
    token: Option<String>,

    /// Output format
    #[arg(short, long, global = true, value_enum)]
    output: Option<OutputFormat>,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage authentication
    Auth {
        #[command(subcommand)]
        action: AuthCommand,
    },
    /// List, view, and download documents
    #[command(alias = "doc")]
    Documents {
        #[command(subcommand)]
        action: DocumentCommand,
    },
    /// Search documents
    Search {
        /// Search query
        query: String,
        /// Maximum number of results (0 for unlimited)
        #[arg(short = 'n', long, default_value = "25")]
        limit: usize,
        /// Fetch all results
        #[arg(short, long)]
        all: bool,
    },
    /// List tags
    Tags,
    /// List correspondents
    Correspondents,
    /// List document types
    DocumentTypes,
    /// Show version information
    Version,
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Save server URL and API token
    Login {
        /// Paperless NGX server URL (skip interactive prompt)
        #[arg(long)]
        url: Option<String>,
        /// API token (skip interactive prompt)
        #[arg(long)]
        token: Option<String>,
    },
    /// Remove saved credentials
    Logout,
    /// Show current configuration
    Status,
}

#[derive(Subcommand)]
enum DocumentCommand {
    /// List all documents
    List {
        /// Maximum number of results (0 for unlimited)
        #[arg(short = 'n', long, default_value = "25")]
        limit: usize,
        /// Fetch all results
        #[arg(short, long)]
        all: bool,
    },
    /// Get documents by ID
    Get {
        /// Document IDs
        #[arg(required = true)]
        ids: Vec<u64>,
    },
    /// Open documents in the Paperless-ngx web UI
    Open {
        /// Document IDs
        #[arg(required = true)]
        ids: Vec<u64>,
    },
    /// Show text content of documents
    Content {
        /// Document IDs
        #[arg(required = true)]
        ids: Vec<u64>,
    },
    /// Download document files
    Download {
        /// Document IDs
        #[arg(required = true)]
        ids: Vec<u64>,
        /// Download original file instead of archived version
        #[arg(long)]
        original: bool,
        /// Output file path (only valid with a single ID)
        #[arg(long, alias = "dest")]
        file: Option<PathBuf>,
    },
}

fn init_tracing(verbosity: u8) {
    let filter = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .with_writer(std::io::stderr)
        .init();
}

fn resolve_limit(limit: usize, all: bool) -> Option<usize> {
    if all || limit == 0 { None } else { Some(limit) }
}

fn build_client(
    url: Option<&str>,
    token: Option<&str>,
    output: Option<OutputFormat>,
) -> anyhow::Result<(pngx_client::Client, OutputFormat, config::ValidConfig)> {
    let raw = RawConfig::load(url, token)?;
    let format = output.unwrap_or(raw.output_format);
    let config = raw.validate()?;
    let client = pngx_client::Client::builder(config.url.as_str(), &config.token)
        .timeout(Duration::from_secs(config.timeout))
        .page_size(config.page_size)
        .build()?;
    Ok((client, format, config))
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::Auth { action } => match action {
            AuthCommand::Login { url, token } => {
                commands::auth::login(url.as_deref(), token.as_deref())?;
            }
            AuthCommand::Logout => commands::auth::logout()?,
            AuthCommand::Status => commands::auth::status()?,
        },
        Command::Version => commands::version::print(),
        Command::Documents { action } => {
            let (client, format, config) =
                build_client(cli.url.as_deref(), cli.token.as_deref(), cli.output)?;
            match action {
                DocumentCommand::List { limit, all } => {
                    commands::documents::list(&client, format, resolve_limit(limit, all))?;
                }
                DocumentCommand::Get { ids } => {
                    commands::documents::get(&client, &ids, format)?;
                }
                DocumentCommand::Open { ids } => {
                    commands::documents::open(&config.url, &ids)?;
                }
                DocumentCommand::Content { ids } => {
                    commands::documents::content(&client, &ids)?;
                }
                DocumentCommand::Download {
                    ids,
                    original,
                    file,
                } => {
                    commands::documents::download(&client, &ids, original, file.as_ref())?;
                }
            }
        }
        Command::Search { query, limit, all } => {
            let (client, format, _) =
                build_client(cli.url.as_deref(), cli.token.as_deref(), cli.output)?;
            commands::search::search(&client, &query, format, resolve_limit(limit, all))?;
        }
        Command::Tags => {
            let (client, format, _) =
                build_client(cli.url.as_deref(), cli.token.as_deref(), cli.output)?;
            commands::tags::list(&client, format)?;
        }
        Command::Correspondents => {
            let (client, format, _) =
                build_client(cli.url.as_deref(), cli.token.as_deref(), cli.output)?;
            commands::correspondents::list(&client, format)?;
        }
        Command::DocumentTypes => {
            let (client, format, _) =
                build_client(cli.url.as_deref(), cli.token.as_deref(), cli.output)?;
            commands::document_types::list(&client, format)?;
        }
    }

    Ok(())
}

fn exit_code_for_error(err: &anyhow::Error) -> ExitCode {
    if let Some(api_err) = err.downcast_ref::<ApiError>() {
        match api_err {
            ApiError::Unauthorized => ExitCode::from(2),
            ApiError::NotFound => ExitCode::from(3),
            ApiError::InvalidUrl(_)
            | ApiError::Io(_)
            | ApiError::Network(_)
            | ApiError::Timeout
            | ApiError::SchemeMismatch { .. } => ExitCode::from(4),
            _ => ExitCode::from(1),
        }
    } else {
        ExitCode::from(1)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let code = exit_code_for_error(&err);
            eprintln!("Error: {err:#}");
            code
        }
    }
}
