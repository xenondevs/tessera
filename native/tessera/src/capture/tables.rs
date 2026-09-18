pub use self::generated::BLOCKS;

pub const MINECRAFT: &str = env!("TESSERA_CAPTURE_MINECRAFT");

#[rustfmt::skip]
#[allow(clippy::all, clippy::pedantic, unused_imports)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/capture.rs"));
}
