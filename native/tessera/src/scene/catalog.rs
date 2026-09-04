use crate::resource::ResourceId;
use crate::resource::cache::Caches;
use crate::resource::resource_manager::ResourceManager;
use std::borrow::Cow;

pub struct Catalog {
    pub items: Vec<ResourceId>,
    pub block_states: Vec<ResourceId>,
}

impl Catalog {
    pub async fn discover(rm: &ResourceManager) -> Catalog {
        let items = rm.list("items", ".json").await;
        let block_states = rm.list("blockstates", ".json").await;
        let convert = |paths: Vec<Cow<str>>| {
            paths
                .into_iter()
                .filter_map(|path| ResourceId::from_path(&path).ok().filter(|id| !id.path.starts_with('_')))
                .collect::<Vec<_>>()
        };
        let items = convert(items);
        let block_states = convert(block_states);
        Catalog { items, block_states }
    }

    pub async fn prime_all(&self, caches: &Caches) {
        let items_fut = caches.items.prime(
            &caches.resources,
            &caches.models,
            &self.items,
            caches.diagnostics(),
        );
        let block_states_fut = caches.block_states.prime(
            &caches.resources,
            &caches.models,
            &self.block_states,
            caches.diagnostics(),
        );
        tokio::join!(items_fut, block_states_fut);
        caches.models.prime_textures(&caches.resources, &caches.textures).await;
    }
}
