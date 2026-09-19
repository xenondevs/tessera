pub mod diagnostics;
pub mod scene;
pub mod resource;
pub mod direction;
pub mod util;
pub mod capture;

pub const MINECRAFT_VERSION: &str = env!("TESSERA_CAPTURE_MINECRAFT");

// ignored by the jvm since the jni crate is the actual final library. This is just for dev. The jni
// crate also specifies this.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
