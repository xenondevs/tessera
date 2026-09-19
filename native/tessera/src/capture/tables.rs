pub use self::generated::BLOCKS;

#[rustfmt::skip]
#[allow(clippy::all, clippy::pedantic, unused_imports)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/capture.rs"));
}
