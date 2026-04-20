use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use pngx_client::{
    Client, Correspondent, DocumentType, DocumentVersion, StoragePath, Tag, UploadMetadata,
};
use url::Url;

use crate::output::{FieldFilter, OutputFormat, resolve_documents};
use crate::resolve::{NameOrId, NameResolver};

pub fn list(
    client: &Client,
    format: OutputFormat,
    limit: Option<usize>,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let (docs, total) = client.collect_documents(limit)?;
    let names = NameResolver::fetch(client, fields)?;
    let docs = resolve_documents(&docs, &names);
    super::print_results(format, &docs, total, fields)?;
    Ok(())
}

pub fn get(
    client: &Client,
    ids: &[u64],
    format: OutputFormat,
    fields: Option<&FieldFilter>,
) -> Result<()> {
    let names = NameResolver::fetch(client, fields)?;

    let mut docs = Vec::with_capacity(ids.len());
    for &id in ids {
        docs.push(client.document(id)?);
    }
    let resolved = resolve_documents(&docs, &names);

    if ids.len() == 1 {
        println!("{}", format.format_detail(&resolved[0], fields)?);
    } else {
        match format {
            OutputFormat::Json => {
                let value = serde_json::to_value(&resolved)?;
                let output = match fields {
                    Some(f) => f.filter_json_array(value),
                    None => value,
                };
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
            OutputFormat::Ndjson => {
                println!("{}", format.format_list(&resolved, fields)?);
            }
            OutputFormat::Markdown => {
                for (i, doc) in resolved.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    println!("{}", format.format_detail(doc, fields)?);
                }
            }
        }
    }
    Ok(())
}

pub fn content(client: &Client, ids: &[u64]) -> Result<()> {
    for (i, &id) in ids.iter().enumerate() {
        if ids.len() > 1 {
            if i > 0 {
                println!();
            }
            eprintln!("--- Document {id} ---");
        }
        let text = client.document_content(id)?;
        println!("{text}");
    }
    Ok(())
}

pub fn download(
    client: &Client,
    ids: &[u64],
    original: bool,
    output: Option<&PathBuf>,
) -> Result<()> {
    if output.is_some() && ids.len() > 1 {
        bail!("--file can only be used with a single document ID");
    }

    let version = if original {
        DocumentVersion::Original
    } else {
        DocumentVersion::Archived
    };

    for &id in ids {
        let doc = client.document(id)?;

        let path = if let Some(p) = output {
            p.clone()
        } else {
            let name = doc
                .original_file_name
                .as_deref()
                .unwrap_or(&format!("document-{id}"))
                .to_string();
            PathBuf::from(&name)
                .file_name()
                .map_or_else(|| PathBuf::from(format!("document-{id}")), PathBuf::from)
        };

        let mut file = fs::File::create(&path)
            .with_context(|| format!("failed to create file: {}", path.display()))?;

        let bytes = client.download_document(id, version, &mut file)?;

        eprintln!("Downloaded {bytes} bytes to {}", path.display());
    }
    Ok(())
}

pub fn open(url: &Url, ids: &[u64]) -> Result<()> {
    for &id in ids {
        let doc_url = format!(
            "{}/documents/{}/details",
            url.as_str().trim_end_matches('/'),
            id
        );
        open::that_detached(&doc_url)?;
        eprintln!("Opened {doc_url}");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn upload(
    client: &Client,
    file: &Path,
    title: Option<String>,
    created: Option<jiff::civil::Date>,
    correspondent: Option<&str>,
    document_type: Option<&str>,
    tags: &[String],
    storage_path: Option<&str>,
    archive_serial_number: Option<u64>,
    wait: bool,
    wait_timeout: Duration,
) -> Result<()> {
    let metadata = UploadMetadata {
        title,
        created,
        correspondent: correspondent
            .map(|input| Correspondent::resolve(client, input))
            .transpose()?,
        document_type: document_type
            .map(|input| DocumentType::resolve(client, input))
            .transpose()?,
        storage_path: storage_path
            .map(|input| StoragePath::resolve(client, input))
            .transpose()?,
        tags: tags
            .iter()
            .map(|name| Tag::resolve(client, name))
            .collect::<Result<Vec<_>>>()?,
        archive_serial_number,
    };

    if wait {
        let id = client.upload_document_and_wait(file, &metadata, wait_timeout)?;
        println!("{id}");
    } else {
        let task_uuid = client.upload_document(file, &metadata)?;
        println!("{task_uuid}");
    }
    Ok(())
}
