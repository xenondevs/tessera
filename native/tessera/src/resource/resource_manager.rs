use super::ResourceId;
use super::pack::ResourcePack;
use crate::diagnostics::Diagnostics;
use std::borrow::Cow;

pub struct ResourceManager {
    packs: Vec<ResourcePack>,
    diagnostics: Diagnostics,
}
impl ResourceManager {
    pub fn new(packs: Vec<ResourcePack>, diagnostics: Diagnostics) -> Self {
        Self { packs, diagnostics }
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    pub async fn get_bytes(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        for pack in &self.packs {
            if let Some(bytes) = pack.get_bytes(path).await {
                return Some(bytes);
            }
        }
        None
    }

    pub async fn get_texture_bytes(&self, id: &ResourceId) -> Option<Cow<'static, [u8]>> {
        self.get_bytes(&id.texture_path()).await
    }

    pub async fn get_texture_mcmeta_bytes(&self, id: &ResourceId) -> Option<Cow<'static, [u8]>> {
        self.get_bytes(&id.texture_mcmeta_path()).await
    }

    pub async fn get_palette_bytes(&self, id: &ResourceId) -> Option<Cow<'static, [u8]>> {
        self.get_bytes(&id.palette_path()).await
    }

    pub async fn get_model_bytes(&self, id: &ResourceId) -> Option<Cow<'static, [u8]>> {
        self.get_bytes(&id.model_path()).await
    }

    pub async fn list<'a>(&'a self, prefix: impl Into<Option<&str>>, ext: &str) -> Vec<Cow<'a, str>> {
        let prefix = prefix.into();
        let mut out = Vec::new();
        for pack in &self.packs {
            out.extend(pack.list_prefix(prefix, ext).await);
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub async fn get_atlas_stack(&self, name: &str) -> Vec<Cow<'static, [u8]>> {
        let mut out = Vec::new();
        let path = format!("assets/minecraft/atlases/{name}.json");
        for pack in self.packs.iter().rev() {
            if let Some(bytes) = pack.get_bytes(&path).await {
                out.push(bytes);
            }
        }
        out
    }

    pub async fn read_many(&self, paths: &[String]) -> Vec<Option<Cow<'static, [u8]>>> {
        let mut out: Vec<Option<Cow<'static, [u8]>>> = Vec::new();
        out.resize_with(paths.len(), || None);

        let mut missing: Vec<usize> = (0..paths.len()).collect();
        for pack in &self.packs {
            if missing.is_empty() {
                break;
            }
            let wanted: Vec<&str> = missing.iter().map(|&i| paths[i].as_str()).collect();
            let got = pack.read_many(&wanted).await;

            let mut still_missing = Vec::with_capacity(missing.len());
            for (&slot, bytes) in missing.iter().zip(got) {
                match bytes {
                    Some(b) => out[slot] = Some(b),
                    None => still_missing.push(slot),
                }
            }
            missing = still_missing;
        }
        out
    }
}
