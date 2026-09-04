pub mod diagnostics;
pub mod scene;
pub mod resource;
pub mod direction;
pub mod util;

// ignored by the jvm since the jni crate is the actual final library. This is just for dev. The jni
// crate also specifies this.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
