use crate::util;
use crate::util::{CONCURRENCY, FastHashMap, FastHashSet};
use foldhash::HashMapExt;
use futures_util::{StreamExt, stream};
use libdeflater::Decompressor;
use rawzip::{CompressionMethod, ZipArchive, ZipArchiveEntryWayfinder, ZipSliceArchive};
use rayon::iter::ParallelIterator;
use rayon::prelude::IntoParallelRefIterator;
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Mutex;
use tokio::fs::File;
use tokio::task::JoinError;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/resources"]
struct InternalResources;

#[derive(Debug, thiserror::Error)]
pub enum PackCreationError {
    #[error("IO Error while creating pack: {0}")]
    IoError(#[from] io::Error),
    #[error("Zip Error while creating pack: {0}")]
    ZipError(#[from] rawzip::Error),
    #[error("Given pack path is not a directory")]
    NotADirectory,
    #[error("Unsupported zip compression method {0} for entry {1}")]
    UnsupportedCompression(u16, String),
    #[error("Join error while creating pack: {0}")]
    JoinError(#[from] JoinError),
}

pub struct ZipEntry {
    wayfinder: ZipArchiveEntryWayfinder,
    method: CompressionMethod,
}

pub enum ResourcePack {
    /// Memory-mapped on disk zip file.
    Zip {
        archive: ZipSliceArchive<memmap2::Mmap>,
        entries: FastHashMap<String, ZipEntry>,
    },
    /// On disk directory
    Directory {
        path: PathBuf,
        index: FastHashSet<String>,
    },
    /// Pre-shipped resources
    Internal,
    /// Delegated to 3rd party fs impl
    Delegated(Box<dyn FileSystem>),
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait FileSystem: Send + Sync {
    fn get_bytes<'a>(&'a self, path: &str) -> BoxFuture<'a, Option<Cow<'static, [u8]>>>;

    fn list_prefix<'a>(&'a self, prefix: Option<&str>, ext: &str) -> BoxFuture<'a, Vec<Cow<'a, str>>>;

    fn read_many<'a>(&'a self, paths: &[&str]) -> BoxFuture<'a, Vec<Option<Cow<'static, [u8]>>>>;
}

impl ResourcePack {
    pub async fn new_zip<P: AsRef<Path>, R: Into<Option<String>>>(
        path: P,
        root: R,
    ) -> Result<Self, PackCreationError> {
        // to string and add trailing / if missing
        let root = root.into().map(|mut root| {
            if !root.ends_with('/') {
                root.push('/');
            }
            root
        });
        let file = File::open(path).await?;
        let map = unsafe { memmap2::Mmap::map(&file)? };
        let archive = ZipArchive::from_slice(map)?;

        let mut entries = FastHashMap::new();
        let mut iter = archive.entries();
        while let Some(entry) = iter.next_entry()? {
            let name = match str::from_utf8(entry.file_path().as_bytes()) {
                Ok(name) => name,
                Err(_) => continue,
            };
            match entry.compression_method() {
                method @ (CompressionMethod::STORE | CompressionMethod::DEFLATE) => {
                    let name = if let Some(root) = &root {
                        let Some(stripped) = name.strip_prefix(root) else {
                            continue;
                        };
                        stripped
                    } else {
                        name
                    };
                    entries.insert(name.to_owned(), ZipEntry { wayfinder: entry.wayfinder(), method });
                }
                unsupported => {
                    return Err(PackCreationError::UnsupportedCompression(
                        unsupported.as_u16(),
                        name.to_string(),
                    ));
                }
            }
        }

        Ok(ResourcePack::Zip { archive, entries })
    }

    pub async fn new_dir<P: Into<PathBuf>>(dir: P) -> Result<Self, PackCreationError> {
        let path = dir.into();
        if !path.is_dir() {
            return Err(PackCreationError::NotADirectory);
        }
        let index = {
            let path = path.clone();
            tokio::task::spawn_blocking(move || walk_files(&path)).await?
        };
        Ok(Self::Directory { path, index })
    }

    pub fn new_internal() -> Self {
        Self::Internal
    }

    pub fn new_delegated(fs: Box<dyn FileSystem>) -> Self {
        Self::Delegated(fs)
    }

    pub async fn get_bytes(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        match self {
            Self::Internal => InternalResources::get(path).map(|f| f.data),

            Self::Delegated(fs) => fs.get_bytes(path).await,

            Self::Zip { archive, entries, .. } => {
                let entry = entries.get(path)?;
                read_zip_entry(archive, entry).map(Cow::Owned)
            }

            Self::Directory { path: dir, index: entries } => {
                if !entries.contains(path) {
                    return None;
                }

                tokio::fs::read(&dir.join(path)).await.ok().map(Cow::Owned)
            }
        }
    }

    pub async fn list_prefix<'a>(
        &'a self,
        prefix: impl Into<Option<&str>>,
        extension: &str,
    ) -> Vec<Cow<'a, str>> {
        let prefix = prefix.into().map(|p| p.trim_matches('/')).filter(|p| !p.is_empty());
        let ext = Some(extension.trim_start_matches('.')).filter(|e| !e.is_empty());

        let matches = |path: &str| -> bool {
            let Some(rest) = path.strip_prefix("assets/") else {
                return false;
            };
            let Some((namespace, rel)) = rest.split_once('/') else {
                return false;
            };
            if namespace.is_empty() || rel.is_empty() {
                return false;
            }

            if let Some(dir) = prefix {
                let Some(tail) = rel.strip_prefix(dir).and_then(|tail| tail.strip_prefix('/')) else {
                    return false;
                };

                if tail.is_empty() {
                    return false;
                }
            }

            let Some(ext) = ext else {
                return true;
            };
            let file = rel.rsplit_once('/').map_or(rel, |(_, file)| file);
            file.rsplit_once('.').is_some_and(|(file_name, extension)| {
                !file_name.is_empty() && extension.eq_ignore_ascii_case(ext)
            })
        };

        match self {
            Self::Zip { entries, .. } => entries
                .keys()
                .filter(|k| matches(k))
                .map(|k| Cow::Borrowed(k.as_str()))
                .collect(),
            Self::Internal => InternalResources::iter().filter(|k| matches(k)).collect(),
            Self::Delegated(fs) => fs.list_prefix(prefix, extension).await,
            Self::Directory { index: entries, .. } => entries
                .iter()
                .filter(|p| matches(p))
                .map(|k| Cow::Borrowed(k.as_str()))
                .collect(),
        }
    }

    pub async fn read_many(&self, paths: &[&str]) -> Vec<Option<Cow<'static, [u8]>>> {
        match self {
            Self::Zip { archive, entries, .. } if util::is_multithreaded() => {
                tokio::task::block_in_place(|| {
                    paths
                        .par_iter()
                        .map(|p| entries.get(*p).and_then(|e| read_zip_entry(archive, e)).map(Cow::Owned))
                        .collect()
                })
            }
            Self::Zip { .. } | Self::Internal => {
                let mut out = Vec::with_capacity(paths.len());
                for path in paths {
                    out.push(self.get_bytes(path).await);
                }
                out
            }
            Self::Delegated(fs) => fs.read_many(paths).await,
            Self::Directory { path, index } => read_dir_batched(path, index, paths).await,
        }
    }
}

fn read_zip_entry(archive: &ZipSliceArchive<memmap2::Mmap>, entry: &ZipEntry) -> Option<Vec<u8>> {
    thread_local! {
        static DECOMPRESSOR: RefCell<Decompressor> = RefCell::new(Decompressor::new());
    }
    let data = archive.get_entry(entry.wayfinder).ok()?.data();

    match entry.method {
        CompressionMethod::STORE => Some(data.to_vec()),
        CompressionMethod::DEFLATE => {
            let mut out = vec![0u8; entry.wayfinder.uncompressed_size_hint() as usize];
            let n = DECOMPRESSOR
                .with(|d| d.borrow_mut().deflate_decompress(data, &mut out))
                .ok()?;
            out.truncate(n);
            Some(out)
        }
        _ => unreachable!(), // checked during pack creation
    }
}
fn walk_files(root: &Path) -> FastHashSet<String> {
    fn visit<'s>(root: &'s Path, dir: PathBuf, out: &'s Mutex<Vec<String>>, scope: &rayon::Scope<'s>) {
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        let mut local = Vec::new();
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else { continue };
            let path = entry.path();
            if file_type.is_dir() {
                scope.spawn(move |scope| visit(root, path, out, scope));
            } else if file_type.is_file()
                && let Some(rel) = path.strip_prefix(root).ok().and_then(Path::to_str)
            {
                local.push(rel.replace('\\', "/"));
            }
        }
        if !local.is_empty() {
            out.lock().unwrap_or_else(|e| e.into_inner()).extend(local);
        }
    }

    let out = Mutex::new(Vec::new());
    rayon::scope(|scope| visit(root, root.to_path_buf(), &out, scope));
    out.into_inner().unwrap_or_else(|e| e.into_inner()).into_iter().collect()
}

async fn read_dir_batched(
    dir: &Path,
    index: &FastHashSet<String>,
    paths: &[&str],
) -> Vec<Option<Cow<'static, [u8]>>> {
    const MAX_CHUNK_SIZE: usize = 8;

    if paths.is_empty() {
        return Vec::new();
    }

    let chunk_size = paths.len().div_ceil(*CONCURRENCY).clamp(1, MAX_CHUNK_SIZE);
    let batch_count = paths.len().div_ceil(chunk_size);
    let concurrency = usize::min(*CONCURRENCY, batch_count);

    let mut results = Vec::with_capacity(paths.len());
    results.resize_with(paths.len(), || None);

    stream::iter(results.chunks_mut(chunk_size).zip(paths.chunks(chunk_size)))
        .for_each_concurrent(Some(concurrency), |(out_chunk, chunk)| async move {
            for (slot, path) in out_chunk.iter_mut().zip(chunk.iter()) {
                if !index.contains(*path) {
                    continue;
                }
                *slot = tokio::fs::read(dir.join(path)).await.ok().map(Cow::Owned);
            }
        })
        .await;

    results
}
