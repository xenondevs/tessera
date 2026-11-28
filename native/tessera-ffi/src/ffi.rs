use std::cell::RefCell;
use std::ffi::{CString, c_char};
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::{panic, ptr};

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

pub(crate) fn set_last_error(msg: impl Into<String>) {
    let msg = CString::new(msg.into().replace('\0', "\\0")).expect("\\0 was just replaced");
    LAST_ERROR.with_borrow_mut(|e| *e = Some(msg));
}

pub(crate) fn guard<T>(on_panic: T, body: impl FnOnce() -> T) -> T {
    panic::catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|payload| {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_string());
        set_last_error(msg);
        on_panic
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn tessera_last_error() -> *const c_char {
    LAST_ERROR.with_borrow(|e| e.as_ref().map_or(ptr::null(), |e| e.as_ptr()))
}

/// Runs a native -> JVM upcall, pre-growing the stack on Windows ([JDK-8394484](https://bugs.openjdk.org/browse/JDK-8394484))
#[inline(always)]
pub(crate) fn upcall<T>(body: impl FnOnce() -> T) -> T {
    #[cfg(windows)]
    grow_stack();
    body()
}

#[cfg(windows)]
#[inline(never)]
fn grow_stack() {
    const HEADROOM: usize = 64 * 1024;
    let mut buf = std::mem::MaybeUninit::<[u8; HEADROOM]>::uninit();
    std::hint::black_box(&mut buf);
}
