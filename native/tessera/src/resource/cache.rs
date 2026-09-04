use super::blockstate::cache::BlockStateCache;
use super::item::cache::ItemCache;
use super::model::cache::ModelCache;
use super::resource_manager::ResourceManager;
use super::texture::cache::TextureCache;
use super::tint::ColorMaps;
use crate::diagnostics::Diagnostics;

pub struct Caches {
    pub resources: ResourceManager,
    pub color_maps: ColorMaps,
    pub block_states: BlockStateCache,
    pub items: ItemCache,
    pub models: ModelCache,
    pub textures: TextureCache,
}

impl Caches {
    pub async fn load(rm: ResourceManager) -> Self {
        Self {
            color_maps: ColorMaps::load(&rm, rm.diagnostics()).await,
            resources: rm,
            block_states: BlockStateCache::new(),
            items: ItemCache::new(),
            models: ModelCache::new(),
            textures: TextureCache::new(true),
        }
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        self.resources.diagnostics()
    }
}
