mod commands;
mod config;
mod output;
mod resolve;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{ArgAction, Args, Parser, Subcommand};
use pngx_client::ApiError;
use tracing_subscriber::EnvFilter;

use commands::MatchingAlgorithmArg;
use config::{ConfigError, RawConfig};
use output::OutputFormat;

#[derive(Parser)]
#[command(
    name = "pngx",
    about = "CLI for Paperless NGX",
    long_about = "CLI for Paperless-ngx, the community-maintained document management system that \
        transforms physical documents into a searchable online archive.",
    after_long_help = "GETTING STARTED:\n  \
        pngx auth login              Save server URL and API token\n  \
        pngx auth status             Show config and verify connection\n\n\
        COMMON WORKFLOWS:\n  \
        pngx inbox                   List unprocessed inbox documents\n  \
        pngx search \"invoice 2024\"   Find documents matching a query\n  \
        pngx documents get 42 43     View document details\n  \
        pngx documents content 42    Read document text\n  \
        pngx documents open 42 43    Open in the web UI\n  \
        pngx tags                    List all tags\n\n\
        OUTPUT:\n  \
        Default output is markdown tables. Use -o json for structured output.\n  \
        Use -F to select specific fields (e.g., -F id,title).\n\n\
        EXIT CODES:\n  \
        0  Success\n  \
        1  Server or deserialization error\n  \
        2  Usage error or unauthorized\n  \
        3  Not found\n  \
        4  I/O, network, timeout, or URL error\n  \
        5  Configuration error",
    version
)]
struct Cli {
    /// Paperless NGX server URL
    #[arg(long, global = true)]
    url: Option<String>,

    /// API authentication token
    #[arg(long, global = true)]
    token: Option<String>,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    /// Emit errors as JSON to stderr
    #[arg(long, global = true, env = "PNGX_JSON_ERRORS")]
    json_errors: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct OutputArgs {
    /// Output format
    #[arg(short, long, value_enum)]
    output: Option<OutputFormat>,

    /// Comma-separated list of fields to include (e.g., id,title,correspondent)
    #[arg(short = 'F', long)]
    fields: Option<String>,
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
    /// List inbox documents
    Inbox {
        /// Maximum number of results (0 for unlimited)
        #[arg(short = 'n', long, default_value = "25")]
        limit: usize,
        /// Fetch all results
        #[arg(short, long)]
        all: bool,
        #[command(flatten)]
        output: OutputArgs,
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
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Manage tags (`list` is the default action)
    Tags {
        #[command(subcommand)]
        action: Option<TagsCommand>,
    },
    /// Manage correspondents (`list` is the default action)
    Correspondents {
        #[command(subcommand)]
        action: Option<CorrespondentsCommand>,
    },
    /// Manage document types (`list` is the default action)
    DocumentTypes {
        #[command(subcommand)]
        action: Option<DocumentTypesCommand>,
    },
    /// Manage storage paths (`list` is the default action)
    StoragePaths {
        #[command(subcommand)]
        action: Option<StoragePathsCommand>,
    },
    /// MCP (Model Context Protocol) server
    Mcp {
        #[command(subcommand)]
        action: McpCommand,
    },
    /// Show version information
    Version,
}

#[derive(Subcommand)]
enum TagsCommand {
    /// List tags (default)
    List {
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Create a new tag
    Create {
        /// Tag name
        name: String,
        /// Hex color (e.g. #c02020)
        #[arg(long)]
        color: Option<String>,
        /// Auto-match algorithm
        #[arg(long, value_enum)]
        matching_algorithm: Option<MatchingAlgorithmArg>,
        /// Match expression for the chosen algorithm
        #[arg(long = "match")]
        matches: Option<String>,
        /// Case-insensitive matching
        #[arg(long)]
        is_insensitive: Option<bool>,
        /// Mark documents with this tag as inbox items
        #[arg(long = "is-inbox")]
        is_inbox_tag: Option<bool>,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Update an existing tag (by ID or name)
    Update {
        /// Tag ID or name
        id_or_name: String,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New hex color
        #[arg(long)]
        color: Option<String>,
        #[arg(long, value_enum)]
        matching_algorithm: Option<MatchingAlgorithmArg>,
        #[arg(long = "match")]
        matches: Option<String>,
        #[arg(long)]
        is_insensitive: Option<bool>,
        #[arg(long = "is-inbox")]
        is_inbox_tag: Option<bool>,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Delete a tag (by ID or name)
    Delete {
        /// Tag ID or name
        id_or_name: String,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum CorrespondentsCommand {
    /// List correspondents (default)
    List {
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Create a new correspondent
    Create {
        name: String,
        #[arg(long, value_enum)]
        matching_algorithm: Option<MatchingAlgorithmArg>,
        #[arg(long = "match")]
        matches: Option<String>,
        #[arg(long)]
        is_insensitive: Option<bool>,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Update an existing correspondent (by ID or name)
    Update {
        id_or_name: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum)]
        matching_algorithm: Option<MatchingAlgorithmArg>,
        #[arg(long = "match")]
        matches: Option<String>,
        #[arg(long)]
        is_insensitive: Option<bool>,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Delete a correspondent (by ID or name)
    Delete {
        id_or_name: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum DocumentTypesCommand {
    /// List document types (default)
    List {
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Create a new document type
    Create {
        name: String,
        #[arg(long, value_enum)]
        matching_algorithm: Option<MatchingAlgorithmArg>,
        #[arg(long = "match")]
        matches: Option<String>,
        #[arg(long)]
        is_insensitive: Option<bool>,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Update an existing document type (by ID or name)
    Update {
        id_or_name: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum)]
        matching_algorithm: Option<MatchingAlgorithmArg>,
        #[arg(long = "match")]
        matches: Option<String>,
        #[arg(long)]
        is_insensitive: Option<bool>,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Delete a document type (by ID or name)
    Delete {
        id_or_name: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum StoragePathsCommand {
    /// List storage paths (default)
    List {
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Create a new storage path
    Create {
        name: String,
        /// Path template (e.g. `{{ correspondent }}/{{ created_year }}`)
        path: String,
        #[arg(long, value_enum)]
        matching_algorithm: Option<MatchingAlgorithmArg>,
        #[arg(long = "match")]
        matches: Option<String>,
        #[arg(long)]
        is_insensitive: Option<bool>,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Update an existing storage path (by ID or name)
    Update {
        id_or_name: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long, value_enum)]
        matching_algorithm: Option<MatchingAlgorithmArg>,
        #[arg(long = "match")]
        matches: Option<String>,
        #[arg(long)]
        is_insensitive: Option<bool>,
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Delete a storage path (by ID or name)
    Delete {
        id_or_name: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    /// Start the MCP server over stdio
    Serve,
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
        #[command(flatten)]
        output: OutputArgs,
    },
    /// Get documents by ID
    Get {
        /// Document IDs
        #[arg(required = true)]
        ids: Vec<u64>,
        #[command(flatten)]
        output: OutputArgs,
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
    /// Upload a new document
    Upload {
        /// Path to the file to upload
        file: PathBuf,
        /// Override the document title
        #[arg(long)]
        title: Option<String>,
        /// Override the document creation date (YYYY-MM-DD)
        #[arg(long)]
        created: Option<jiff::civil::Date>,
        /// Assign a correspondent (by ID or exact name)
        #[arg(long)]
        correspondent: Option<String>,
        /// Assign a document type (by ID or exact name)
        #[arg(long)]
        document_type: Option<String>,
        /// Attach tags (comma-separated IDs or exact names)
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// Assign a storage path (by ID or exact name)
        #[arg(long)]
        storage_path: Option<String>,
        /// Archive serial number
        #[arg(long = "asn")]
        archive_serial_number: Option<u64>,
        /// Wait for consumption to finish and print the created document ID
        #[arg(short = 'w', long)]
        wait: bool,
        /// Maximum seconds to wait when `--wait` is set
        #[arg(long, default_value = "120")]
        wait_timeout: u64,
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
) -> anyhow::Result<(pngx_client::Client, config::ValidConfig)> {
    let raw = RawConfig::load(url, token)?;
    let config = raw.validate()?;
    let client = pngx_client::Client::builder(config.url.as_str(), &config.token)
        .timeout(Duration::from_secs(config.timeout))
        .page_size(config.page_size)
        .build()?;
    Ok((client, config))
}

fn resolve_output(output: &OutputArgs, config: &config::ValidConfig) -> OutputFormat {
    output.output.unwrap_or(config.output_format)
}

/// Parse and validate the `--fields` flag for a specific entity type.
fn resolve_fields<T: output::FieldNames>(
    output: &OutputArgs,
) -> anyhow::Result<Option<output::FieldFilter>> {
    match &output.fields {
        Some(raw) => {
            let filter = output::FieldFilter::parse::<T>(raw)?;
            Ok(Some(filter))
        }
        None => Ok(None),
    }
}

#[allow(clippy::too_many_lines)]
fn run(cli: Cli) -> anyhow::Result<()> {
    init_tracing(cli.verbose);

    match cli.command {
        Command::Auth { action } => match action {
            AuthCommand::Login { url, token } => {
                commands::auth::login(url.as_deref(), token.as_deref())?;
            }
            AuthCommand::Logout => commands::auth::logout()?,
            AuthCommand::Status => {
                commands::auth::status(cli.url.as_deref(), cli.token.as_deref())?;
            }
        },
        Command::Version => {
            commands::version::print(cli.url.as_deref(), cli.token.as_deref())?;
        }
        Command::Documents { action } => {
            let (client, config) = build_client(cli.url.as_deref(), cli.token.as_deref())?;
            match action {
                DocumentCommand::List { limit, all, output } => {
                    let format = resolve_output(&output, &config);
                    let fields = resolve_fields::<output::ResolvedDocument>(&output)?;
                    commands::documents::list(
                        &client,
                        format,
                        resolve_limit(limit, all),
                        fields.as_ref(),
                    )?;
                }
                DocumentCommand::Get { ids, output } => {
                    let format = resolve_output(&output, &config);
                    let fields = resolve_fields::<output::ResolvedDocument>(&output)?;
                    commands::documents::get(&client, &ids, format, fields.as_ref())?;
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
                DocumentCommand::Upload {
                    file,
                    title,
                    created,
                    correspondent,
                    document_type,
                    tags,
                    storage_path,
                    archive_serial_number,
                    wait,
                    wait_timeout,
                } => {
                    commands::documents::upload(
                        &client,
                        &file,
                        title,
                        created,
                        correspondent.as_deref(),
                        document_type.as_deref(),
                        &tags,
                        storage_path.as_deref(),
                        archive_serial_number,
                        wait,
                        Duration::from_secs(wait_timeout),
                    )?;
                }
            }
        }
        Command::Inbox { limit, all, output } => {
            let (client, config) = build_client(cli.url.as_deref(), cli.token.as_deref())?;
            let format = resolve_output(&output, &config);
            let fields = resolve_fields::<output::ResolvedDocument>(&output)?;
            commands::inbox::list(&client, format, resolve_limit(limit, all), fields.as_ref())?;
        }
        Command::Search {
            query,
            limit,
            all,
            output,
        } => {
            let (client, config) = build_client(cli.url.as_deref(), cli.token.as_deref())?;
            let format = resolve_output(&output, &config);
            let fields = resolve_fields::<output::ResolvedDocument>(&output)?;
            commands::search::search(
                &client,
                &query,
                format,
                resolve_limit(limit, all),
                fields.as_ref(),
            )?;
        }
        Command::Tags { action } => {
            let (client, config) = build_client(cli.url.as_deref(), cli.token.as_deref())?;
            dispatch_tags(&client, &config, action)?;
        }
        Command::Correspondents { action } => {
            let (client, config) = build_client(cli.url.as_deref(), cli.token.as_deref())?;
            dispatch_correspondents(&client, &config, action)?;
        }
        Command::DocumentTypes { action } => {
            let (client, config) = build_client(cli.url.as_deref(), cli.token.as_deref())?;
            dispatch_document_types(&client, &config, action)?;
        }
        Command::StoragePaths { action } => {
            let (client, config) = build_client(cli.url.as_deref(), cli.token.as_deref())?;
            dispatch_storage_paths(&client, &config, action)?;
        }
        Command::Mcp { action } => match action {
            McpCommand::Serve => {
                let (client, _config) = build_client(cli.url.as_deref(), cli.token.as_deref())?;
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?
                    .block_on(commands::mcp::serve(client))?;
            }
        },
    }

    Ok(())
}

fn dispatch_tags(
    client: &pngx_client::Client,
    config: &config::ValidConfig,
    action: Option<TagsCommand>,
) -> anyhow::Result<()> {
    match action.unwrap_or(TagsCommand::List {
        output: OutputArgs {
            output: None,
            fields: None,
        },
    }) {
        TagsCommand::List { output } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::Tag>(&output)?;
            commands::tags::list(client, format, fields.as_ref())
        }
        TagsCommand::Create {
            name,
            color,
            matching_algorithm,
            matches,
            is_insensitive,
            is_inbox_tag,
            output,
        } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::Tag>(&output)?;
            commands::tags::create(
                client,
                name,
                color,
                matching_algorithm.map(Into::into),
                matches,
                is_insensitive,
                is_inbox_tag,
                format,
                fields.as_ref(),
            )
        }
        TagsCommand::Update {
            id_or_name,
            name,
            color,
            matching_algorithm,
            matches,
            is_insensitive,
            is_inbox_tag,
            output,
        } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::Tag>(&output)?;
            commands::tags::update(
                client,
                &id_or_name,
                name,
                color,
                matching_algorithm.map(Into::into),
                matches,
                is_insensitive,
                is_inbox_tag,
                format,
                fields.as_ref(),
            )
        }
        TagsCommand::Delete { id_or_name, yes } => {
            confirm_delete("tag", &id_or_name, yes)?;
            commands::tags::delete(client, &id_or_name)
        }
    }
}

fn dispatch_correspondents(
    client: &pngx_client::Client,
    config: &config::ValidConfig,
    action: Option<CorrespondentsCommand>,
) -> anyhow::Result<()> {
    match action.unwrap_or(CorrespondentsCommand::List {
        output: OutputArgs {
            output: None,
            fields: None,
        },
    }) {
        CorrespondentsCommand::List { output } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::Correspondent>(&output)?;
            commands::correspondents::list(client, format, fields.as_ref())
        }
        CorrespondentsCommand::Create {
            name,
            matching_algorithm,
            matches,
            is_insensitive,
            output,
        } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::Correspondent>(&output)?;
            commands::correspondents::create(
                client,
                name,
                matching_algorithm.map(Into::into),
                matches,
                is_insensitive,
                format,
                fields.as_ref(),
            )
        }
        CorrespondentsCommand::Update {
            id_or_name,
            name,
            matching_algorithm,
            matches,
            is_insensitive,
            output,
        } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::Correspondent>(&output)?;
            commands::correspondents::update(
                client,
                &id_or_name,
                name,
                matching_algorithm.map(Into::into),
                matches,
                is_insensitive,
                format,
                fields.as_ref(),
            )
        }
        CorrespondentsCommand::Delete { id_or_name, yes } => {
            confirm_delete("correspondent", &id_or_name, yes)?;
            commands::correspondents::delete(client, &id_or_name)
        }
    }
}

fn dispatch_document_types(
    client: &pngx_client::Client,
    config: &config::ValidConfig,
    action: Option<DocumentTypesCommand>,
) -> anyhow::Result<()> {
    match action.unwrap_or(DocumentTypesCommand::List {
        output: OutputArgs {
            output: None,
            fields: None,
        },
    }) {
        DocumentTypesCommand::List { output } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::DocumentType>(&output)?;
            commands::document_types::list(client, format, fields.as_ref())
        }
        DocumentTypesCommand::Create {
            name,
            matching_algorithm,
            matches,
            is_insensitive,
            output,
        } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::DocumentType>(&output)?;
            commands::document_types::create(
                client,
                name,
                matching_algorithm.map(Into::into),
                matches,
                is_insensitive,
                format,
                fields.as_ref(),
            )
        }
        DocumentTypesCommand::Update {
            id_or_name,
            name,
            matching_algorithm,
            matches,
            is_insensitive,
            output,
        } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::DocumentType>(&output)?;
            commands::document_types::update(
                client,
                &id_or_name,
                name,
                matching_algorithm.map(Into::into),
                matches,
                is_insensitive,
                format,
                fields.as_ref(),
            )
        }
        DocumentTypesCommand::Delete { id_or_name, yes } => {
            confirm_delete("document type", &id_or_name, yes)?;
            commands::document_types::delete(client, &id_or_name)
        }
    }
}

fn dispatch_storage_paths(
    client: &pngx_client::Client,
    config: &config::ValidConfig,
    action: Option<StoragePathsCommand>,
) -> anyhow::Result<()> {
    match action.unwrap_or(StoragePathsCommand::List {
        output: OutputArgs {
            output: None,
            fields: None,
        },
    }) {
        StoragePathsCommand::List { output } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::StoragePath>(&output)?;
            commands::storage_paths::list(client, format, fields.as_ref())
        }
        StoragePathsCommand::Create {
            name,
            path,
            matching_algorithm,
            matches,
            is_insensitive,
            output,
        } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::StoragePath>(&output)?;
            commands::storage_paths::create(
                client,
                name,
                path,
                matching_algorithm.map(Into::into),
                matches,
                is_insensitive,
                format,
                fields.as_ref(),
            )
        }
        StoragePathsCommand::Update {
            id_or_name,
            name,
            path,
            matching_algorithm,
            matches,
            is_insensitive,
            output,
        } => {
            let format = resolve_output(&output, config);
            let fields = resolve_fields::<pngx_client::StoragePath>(&output)?;
            commands::storage_paths::update(
                client,
                &id_or_name,
                name,
                path,
                matching_algorithm.map(Into::into),
                matches,
                is_insensitive,
                format,
                fields.as_ref(),
            )
        }
        StoragePathsCommand::Delete { id_or_name, yes } => {
            confirm_delete("storage path", &id_or_name, yes)?;
            commands::storage_paths::delete(client, &id_or_name)
        }
    }
}

/// Prompt for delete confirmation when stdin is a TTY; in non-interactive
/// contexts, require `--yes`.
fn confirm_delete(entity: &str, id_or_name: &str, yes: bool) -> anyhow::Result<()> {
    use std::io::{self, IsTerminal, Write};

    if yes {
        return Ok(());
    }
    if io::stdin().is_terminal() {
        eprint!("Delete {entity} '{id_or_name}'? [y/N] ");
        io::stderr().flush().ok();
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        if matches!(response.trim(), "y" | "Y" | "yes") {
            Ok(())
        } else {
            anyhow::bail!("delete aborted");
        }
    } else {
        anyhow::bail!(
            "refusing to delete {entity} '{id_or_name}' without --yes in a non-interactive context"
        );
    }
}

/// Map an error to a machine-readable error code string.
fn error_code(err: &anyhow::Error) -> &'static str {
    if let Some(api_err) = err.downcast_ref::<ApiError>() {
        match api_err {
            ApiError::Unauthorized => "unauthorized",
            ApiError::NotFound => "not_found",
            ApiError::BadRequest { .. } => "bad_request",
            ApiError::ValidationError { .. } => "validation_error",
            ApiError::TaskPending { .. } => "task_pending",
            ApiError::TaskUnknown { .. } => "task_unknown",
            ApiError::InvalidUrl(_) => "invalid_url",
            ApiError::Io(_) => "io_error",
            ApiError::Network(_) => "network_error",
            ApiError::Timeout => "timeout",
            ApiError::SchemeMismatch { .. } => "scheme_mismatch",
            ApiError::Server { .. } => "server_error",
            ApiError::Deserialization(_) => "deserialization_error",
        }
    } else if err.downcast_ref::<ConfigError>().is_some() {
        "config_error"
    } else if err.downcast_ref::<output::FieldFilterError>().is_some() {
        "usage_error"
    } else {
        "internal_error"
    }
}

fn exit_code_for_error(err: &anyhow::Error) -> ExitCode {
    if let Some(api_err) = err.downcast_ref::<ApiError>() {
        match api_err {
            ApiError::Unauthorized
            | ApiError::BadRequest { .. }
            | ApiError::ValidationError { .. } => ExitCode::from(2),
            ApiError::NotFound => ExitCode::from(3),
            ApiError::InvalidUrl(_)
            | ApiError::Io(_)
            | ApiError::Network(_)
            | ApiError::Timeout
            | ApiError::SchemeMismatch { .. }
            | ApiError::TaskPending { .. }
            | ApiError::TaskUnknown { .. } => ExitCode::from(4),
            _ => ExitCode::from(1),
        }
    } else if err.downcast_ref::<ConfigError>().is_some() {
        ExitCode::from(5)
    } else if err.downcast_ref::<output::FieldFilterError>().is_some() {
        ExitCode::from(2)
    } else {
        ExitCode::from(1)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json_errors = cli.json_errors;

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let code = exit_code_for_error(&err);
            if json_errors {
                let json_err = serde_json::json!({
                    "error": format!("{err:#}"),
                    "code": error_code(&err),
                });
                eprintln!("{json_err}");
            } else {
                eprintln!("Error: {err:#}");
            }
            code
        }
    }
}
