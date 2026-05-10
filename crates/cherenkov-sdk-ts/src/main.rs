//! TypeScript SDK + AsyncAPI generator CLI for Cherenkov.
//!
//! Reads a YAML server config (the same format `cherenkov-server`
//! consumes), pulls the namespaces / schemas section, and emits one of
//! two artifacts:
//!
//! * `--target ts` — TypeScript types + a thin `publish/subscribe`
//!   wrapper, derived from each namespace's JSON Schema.
//! * `--target asyncapi` — AsyncAPI 2.6 document covering the same
//!   namespaces, useful for portal documentation and Postman import.
//!
//! The generator is a small, opinionated bridge for getting started.
//! Production users with non-trivial schemas should plug a dedicated
//! generator (`asyncapi-generator`, `openapi-typescript`) into the
//! same input file.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use serde_json::Value;

mod asyncapi;
mod tsgen;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Target {
    Ts,
    Asyncapi,
}

#[derive(Debug, Parser)]
#[command(
    name = "cherenkov-sdk-ts",
    about = "Generate TypeScript SDK or AsyncAPI documents from a Cherenkov server config",
    version
)]
struct Cli {
    /// Path to the server config YAML.
    #[arg(short, long)]
    config: PathBuf,
    /// Output target.
    #[arg(short, long, value_enum, default_value_t = Target::Ts)]
    target: Target,
    /// Where to write the generated artifact. Defaults to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ConfigDocument {
    #[serde(default)]
    namespaces: serde_yaml::Mapping,
}

#[derive(Debug, Deserialize)]
struct NamespaceSection {
    #[serde(default)]
    kind: Option<String>,
    schema: Option<Value>,
    #[serde(default)]
    schema_path: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_text =
        fs::read_to_string(&cli.config).with_context(|| format!("read {:?}", cli.config))?;
    let doc: ConfigDocument = serde_yaml::from_str(&config_text).context("parse config yaml")?;

    let mut namespaces: Vec<(String, Value)> = Vec::new();
    let config_dir = cli
        .config
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    for (key, value) in doc.namespaces.iter() {
        let name = key
            .as_str()
            .context("namespace key must be a string")?
            .to_owned();
        let section: NamespaceSection =
            serde_yaml::from_value(value.clone()).context("parse namespace section")?;
        let schema = match (section.schema, section.schema_path) {
            (Some(inline), None) => inline,
            (None, Some(path)) => {
                let resolved = if path.is_absolute() {
                    path
                } else {
                    config_dir.join(path)
                };
                let text =
                    fs::read_to_string(&resolved).with_context(|| format!("read {resolved:?}"))?;
                serde_json::from_str(&text).with_context(|| format!("parse {resolved:?}"))?
            }
            (Some(_), Some(_)) => {
                anyhow::bail!("namespace `{name}`: schema and schema_path are mutually exclusive");
            }
            (None, None) => continue,
        };
        let _ = section.kind; // honored for forward-compat; only json-schema today.
        namespaces.push((name, schema));
    }

    let rendered = match cli.target {
        Target::Ts => tsgen::render(&namespaces),
        Target::Asyncapi => asyncapi::render(&namespaces)?,
    };

    if let Some(path) = cli.output {
        fs::write(&path, rendered).with_context(|| format!("write {path:?}"))?;
    } else {
        println!("{rendered}");
    }
    Ok(())
}
