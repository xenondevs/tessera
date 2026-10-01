use std::{ptr, slice};
use std::sync::Arc;
use tokio::runtime::{Builder, Runtime};
use tessera::diagnostics::Diagnostics;
use tessera::resource::cache::Caches;
use tessera::resource::pack::ResourcePack;
use tessera::resource::resource_manager::ResourceManager;
use tessera::scene::catalog::Catalog;
use tessera::scene::Renderer;
use crate::ffi;

pub struct NativeRenderer {
    // decl order matters cause runtime needs to shut down in order for the last renderer arc to be the one below
    pub(crate) runtime: Runtime,
    pub(crate) inner: Arc<Renderer>,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_create_renderer(packs: *const *mut ResourcePack, count: usize) -> *mut NativeRenderer {
    // SAFETY: JVM passes an arena allocated array of count ptrs that stays alive for this call
    let packs: Vec<ResourcePack> = unsafe { slice::from_raw_parts(packs, count) }
        .iter()
        // SAFETY: each pointer is a live Box::into_raw from a tessera_create_*_pack export and appears only once
        .map(|&p| *unsafe { Box::from_raw(p) })
        .collect();

    ffi::guard(ptr::null_mut(), || {
        let runtime = Builder::new_multi_thread().build().expect("rt construction cant fail");
        let renderer = runtime.block_on(async {
            let rm = ResourceManager::new(packs, Diagnostics::new());
            let catalog = Catalog::discover(&rm).await;
            let caches = Caches::load(rm).await;
            catalog.prime_all(&caches).await;
            Arc::new(Renderer::new(caches))
        });
        Box::into_raw(Box::new(NativeRenderer { runtime, inner: renderer }))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_destroy_renderer(renderer: *mut NativeRenderer) {
    // SAFETY: pointer came from tessera_create_renderer above and the JVM still owns it
    drop(unsafe { Box::from_raw(renderer) });
}
