mod states;
pub mod pool;
pub mod emit;

use self::pool::Pools;
use crate::raw::RawBlockFile;
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
    MinecraftVersion {
        path: PathBuf,
        found: String,
        expected: String,
    },
    #[error("Unexpected discards:\n{0}")]
    UnexpectedDiscards(String),
    #[error("{} is invalid: {message}", .path.display())]
    Invalid { path: PathBuf, message: String },
    #[error("block {id} has duplicate entries")]
    DuplicateBlock { id: String },
}

pub fn generate(input: &Path, minecraft_version: &str, out: &Path) -> Result<Vec<String>, GenerateError> {
    fn parse<T: DeserializeOwned>(path: &Path) -> Result<T, GenerateError> {
        let mut bytes =
            fs::read(path).map_err(|err| GenerateError::Io { path: path.to_owned(), source: err })?;
        simd_json::serde::from_slice(&mut bytes)
            .map_err(|err| GenerateError::Parse { path: path.to_owned(), message: err.to_string() })
    }

    let mut files = Vec::new();

    let mut queue = vec![input.join("blocks")];
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

    let expected_fails: BTreeMap<String, BTreeSet<String>> = parse(&input.join("ignored_discards.json"))?;
    let mut matched = BTreeSet::<(String, String)>::new();
    let mut unexpected = Vec::new();

    let mut pools = Pools::default();
    for path in files {
        let file: RawBlockFile = parse(&path)?;
        if file.format != FORMAT {
            return Err(GenerateError::Format { path, found: file.format });
        }

        if file.minecraft_version != minecraft_version {
            return Err(GenerateError::MinecraftVersion {
                path,
                found: file.minecraft_version,
                expected: minecraft_version.to_string(),
            });
        }

        for (key, state) in &file.states {
            for reason in state.discarded.iter().flatten() {
                if let Some(reasons) = expected_fails.get(&file.block)
                    && reasons.contains(reason)
                {
                    matched.insert((file.block.clone(), reason.clone()));
                } else {
                    unexpected.push(format!("  {}[{key}]: {reason}", file.block))
                }
            }
        }

        pools.block(&file, &path)?
    }

    if !unexpected.is_empty() {
        return Err(GenerateError::UnexpectedDiscards(unexpected.join("\n")));
    }

    let stale = expected_fails
        .iter()
        .flat_map(|(block, reasons)| reasons.iter().map(move |reason| (block, reason)))
        .filter(|(block, reason)| !matched.contains(&((*block).clone(), (*reason).clone())))
        .map(|(block, reason)| format!("{block}: {reason}"))
        .collect();

    pools.finish()?;
    let generated = out.join("capture.rs");
    fs::write(&generated, emit::emit(&pools))
        .map_err(|err| GenerateError::Io { path: generated, source: err })?;
    Ok(stale)
}
