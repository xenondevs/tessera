use crate::ffi::{guard, set_last_error, BoxFuture};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::borrow::Cow;
use std::future::ready;
use std::{ptr, slice};
use tessera::resource::pack::{FileSystem, PathFilter, ResourcePack};
use tessera::util::FastHashSet;

pub(crate) type ReadFn = unsafe extern "C" fn(path: *const u8, path_len: usize, sink: *mut Vec<u8>) -> i64;
pub(crate) type ListFn = unsafe extern "C" fn(sink: *mut Vec<String>) -> i32;

struct JvmFileSystem {
    read: ReadFn,
    index: FastHashSet<String>,
}

impl JvmFileSystem {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        if !self.index.contains(path) {
            return None;
        }
        let mut sink = Vec::new();
        // SAFETY: stub lives in the jvm pack's arena which is valid while the renderer owns a ref to the pack
        let count = unsafe { (self.read)(path.as_ptr(), path.len(), &mut sink) };
        // -1 means the jvm threw an exception and is waiting until we're done
        let count = usize::try_from(count).ok()?;
        assert!(
            count <= sink.capacity(),
            "Java side reported invalid bytes for {path}: {count} for {} reserved",
            sink.capacity()
        );
        // SAFETY: java wrote count amount of bytes into the capacity
        unsafe { sink.set_len(count) }
        Some(sink)
    }
}

impl FileSystem for JvmFileSystem {
    fn get_bytes<'a>(&'a self, path: &str) -> BoxFuture<'a, Option<Cow<'static, [u8]>>> {
        Box::pin(ready(self.read(path).map(Cow::Owned)))
    }

    fn list_prefix<'a>(&'a self, prefix: Option<&str>, ext: &str) -> BoxFuture<'a, Vec<Cow<'a, str>>> {
        let filter = PathFilter::new(prefix, ext);
        Box::pin(ready(
            self.index
                .iter()
                .filter(|p| filter.matches(p))
                .map(|p| Cow::Borrowed(p.as_str()))
                .collect(),
        ))
    }

    fn read_many<'a>(&'a self, paths: &[&str]) -> BoxFuture<'a, Vec<Option<Cow<'static, [u8]>>>> {
        let out = tokio::task::block_in_place(|| paths.par_iter().map(|p| self.read(p).map(Cow::Owned)).collect());
        Box::pin(ready(out))
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_create_zip_pack(
    path_ptr: *const u8,
    path_len: usize,
    root_ptr: *const u8,
    root_len: usize,
) -> *mut ResourcePack {
    // SAFETY: Java passes a live segment of len bytes. Arena::allocateFrom encodes UTF-8 so the
    // bytes are always valid UTF-8
    let path = unsafe { str::from_utf8_unchecked(slice::from_raw_parts(path_ptr, path_len)) };
    let root = (!root_ptr.is_null())
        .then(|| unsafe { str::from_utf8_unchecked(slice::from_raw_parts(root_ptr, root_len)) }.to_string());
    guard(ptr::null_mut(), || match ResourcePack::new_zip(path, root) {
        Ok(pack) => Box::into_raw(Box::new(pack)),
        Err(e) => {
            set_last_error(e.to_string());
            ptr::null_mut()
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_create_directory_pack(
    path_ptr: *const u8,
    path_len: usize,
) -> *mut ResourcePack {
    // SAFETY: Java passes a live segment of len bytes. Arena::allocateFrom encodes UTF-8 so the
    // bytes are always valid UTF-8
    let path = unsafe { str::from_utf8_unchecked(slice::from_raw_parts(path_ptr, path_len)) };
    guard(ptr::null_mut(), || match ResourcePack::new_dir(path) {
        Ok(pack) => Box::into_raw(Box::new(pack)),
        Err(e) => {
            set_last_error(e.to_string());
            ptr::null_mut()
        }
    })
}


#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_create_jvm_pack(read: ReadFn, list: ListFn) -> *mut ResourcePack {
    guard(ptr::null_mut(), || {
        let mut paths = Vec::new();
        // SAFETY: stub lives in the jvm pack's arena which is valid while the renderer owns a ref to the pack
        if unsafe { list(&mut paths) } < 0 {
            return ptr::null_mut();
        }
        let fs = JvmFileSystem { read, index: paths.into_iter().collect() };
        Box::into_raw(Box::new(ResourcePack::new_delegated(Box::new(fs))))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_reserve_sink(sink: *mut Vec<u8>, len: usize) -> *mut u8 {
    // SAFETY: Sink is the Vec JvmFileSystem::read gave to the upcall that is in turn calling this
    let sink = unsafe { &mut *sink };
    sink.clear();
    sink.reserve_exact(len);
    sink.as_mut_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_push_string(sink: *mut Vec<String>, ptr: *const u8, len: usize) {
    // SAFETY: Java passes a live segment of len bytes. Arena::allocateFrom encodes UTF-8 so the
    // bytes are always valid UTF-8
    let str = unsafe { str::from_utf8_unchecked(slice::from_raw_parts(ptr, len)) };
    unsafe { &mut *sink }.push(str.to_owned())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn tessera_destroy_pack(pack: *mut ResourcePack) {
    // SAFETY: pack came from one of the tessera_create_*_pack methods above and the JVM still owns it
    drop(unsafe { Box::from_raw(pack) });
}
