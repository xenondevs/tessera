pub(crate) mod pack;
pub(crate) mod ffi;
pub(crate) mod renderer;
pub(crate) mod render;
pub(crate) mod encode;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
