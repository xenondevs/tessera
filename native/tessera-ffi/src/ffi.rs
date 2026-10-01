use std::cell::RefCell;
use std::ffi::{c_char, CString};
use std::{panic, ptr};
use std::panic::AssertUnwindSafe;
use std::pin::Pin;

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output=T> + Send + 'a>>;

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
