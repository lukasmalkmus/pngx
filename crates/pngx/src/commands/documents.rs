use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use pngx_client::{Client, DocumentVersion};
use url::Url;

use crate::output::{OutputFormat, resolve_documents};
use crate::resolve::NameResolver;

pub fn list(client: &Client, format: OutputFormat, limit: Option<usize>) -> Result<()> {
    let (docs, total) = client.collect_documents(limit)?;
    let names = NameResolver::fetch(client)?;
    let docs = resolve_documents(&docs, &names);
    super::print_results(format, &docs, total)?;
    Ok(())
}

pub fn get(client: &Client, ids: &[u64], format: OutputFormat) -> Result<()> {
    let names = NameResolver::fetch(client)?;

    let mut docs = Vec::with_capacity(ids.len());
    for &id in ids {
        docs.push(client.document(id)?);
    }
    let resolved = resolve_documents(&docs, &names);

    if ids.len() == 1 {
        println!("{}", format.format_detail(&resolved[0])?);
    } else {
        match format {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&resolved)?);
            }
            OutputFormat::Markdown => {
                for (i, doc) in resolved.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    println!("{}", format.format_detail(doc)?);
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
