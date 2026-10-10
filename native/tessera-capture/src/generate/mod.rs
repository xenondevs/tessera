mod states;
pub mod pool;
pub mod emit;

use self::pool::Pools;
use crate::raw::{RawBlockFile, RawItemFile};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const FORMAT: u32 = 1;

#[derive(Debug, Error)]
pub enum GenerateError {
    #[error("Failed to read or write {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to parse {}: {message}", .path.display())]
    Parse { path: PathBuf, message: String },
    #[error("{} has wrong format number {found}, expected {FORMAT}", .path.display())]
    Format { path: PathBuf, found: u32 },
    #[error("{} has wrong minecraft version {found}, expected {expected}", .path.display())]
    MinecraftVersion { path: PathBuf, found: String, expected: String },
    #[error("Unexpected discards:\n{0}")]
    UnexpectedDiscards(String),
    #[error("{} is invalid: {message}", .path.display())]
    Invalid { path: PathBuf, message: String },
    #[error("{kind} {id} has duplicate entries")]
    Duplicate { kind: &'static str, id: String },
}

#[derive(Deserialize)]
struct Ignored {
    #[serde(default)]
    blocks: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    items: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Default)]
struct Discards {
    kind: &'static str,
    ignored: BTreeMap<String, BTreeSet<String>>,
    matched: BTreeSet<(String, String)>,
    unexpected: Vec<String>,
}

impl Discards {
    fn record(&mut self, id: &str, at: &str, reasons: &[String]) {
        for reason in reasons {
            if let Some(reasons) = self.ignored.get(id)
                && reasons.contains(reason)
            {
                self.matched.insert((id.to_owned(), reason.clone()));
            } else {
                self.unexpected.push(format!("  {} {id}[{at}]: {reason}", self.kind))
            }
        }
    }

    fn stale(&self) -> impl Iterator<Item = String> + '_ {
        self.ignored
            .iter()
            .flat_map(|(id, reasons)| reasons.iter().map(move |reason| (id, reason)))
            .filter(|(id, reason)| !self.matched.contains(&((*id).clone(), (*reason).clone())))
            .map(|(id, reason)| format!("{} {id}: {reason}", self.kind))
    }
}

fn check_file(path: &Path, format: u32, version: &str, expected_version: &str) -> Result<(), GenerateError> {
    if format != FORMAT {
        return Err(GenerateError::Format { path: path.to_owned(), found: format });
    }
    if version != expected_version {
        return Err(GenerateError::MinecraftVersion {
            path: path.to_owned(),
            found: version.to_owned(),
            expected: expected_version.to_owned(),
        });
    }
    Ok(())
}

pub fn generate(input: &Path, minecraft_version: &str, out: &Path) -> Result<Vec<String>, GenerateError> {
    fn parse<T: DeserializeOwned>(path: &Path) -> Result<T, GenerateError> {
        let mut bytes = fs::read(path).map_err(|err| GenerateError::Io { path: path.to_owned(), source: err })?;
        simd_json::serde::from_slice(&mut bytes)
            .map_err(|err| GenerateError::Parse { path: path.to_owned(), message: err.to_string() })
    }

    fn files(root: &Path) -> Result<Vec<PathBuf>, GenerateError> {
        let mut files = Vec::new();

        let mut queue = vec![root.to_owned()];
        while let Some(dir) = queue.pop() {
            let io_err = |err| GenerateError::Io { path: dir.clone(), source: err };
            for entry in fs::read_dir(&dir).map_err(io_err)? {
                let path = entry.map_err(io_err)?.path();
                if path.is_dir() {
                    queue.push(path)
                } else if path.extension().is_some_and(|ext| ext == "json") {
                    files.push(path)
                }
            }
        }
        files.sort();
        Ok(files)
    }

    let ignored_fail: Ignored = parse(&input.join("ignored_discards.json"))?;
    let mut block_discards = Discards { kind: "block", ignored: ignored_fail.blocks, ..Default::default() };
    let mut item_discards = Discards { kind: "item", ignored: ignored_fail.items, ..Default::default() };

    let mut pools = Pools::default();
    for path in files(&input.join("blocks"))? {
        let file: RawBlockFile = parse(&path)?;
        check_file(&path, file.format, &file.minecraft_version, minecraft_version)?;

        for (key, state) in &file.states {
            block_discards.record(&file.block, &format!("[{key}]"), state.discarded.as_deref().unwrap_or_default());
        }
        pools.block(&file, &path)?;
    }
    for path in files(&input.join("items"))? {
        let file: RawItemFile = parse(&path)?;
        check_file(&path, file.format, &file.minecraft_version, minecraft_version)?;
        item_discards.record(&file.item, "", file.default_leaf.discarded.as_deref().unwrap_or_default());
        pools.item(&file, &path)?;
    }

    let unexpected = block_discards
        .unexpected
        .iter()
        .chain(&item_discards.unexpected)
        .map(String::as_str)
        .collect::<Vec<_>>();

    if !unexpected.is_empty() {
        return Err(GenerateError::UnexpectedDiscards(unexpected.join("\n")));
    }

    let stale = block_discards.stale().chain(item_discards.stale()).collect();

    pools.finish()?;
    let generated = out.join("capture.rs");
    fs::write(&generated, emit::emit(&pools)).map_err(|err| GenerateError::Io { path: generated, source: err })?;
    Ok(stale)
}
