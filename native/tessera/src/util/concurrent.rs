use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use std::sync::LazyLock;
use tokio::runtime::{Handle, RuntimeFlavor};

pub(crate) static CONCURRENCY: LazyLock<usize> =
    LazyLock::new(|| std::thread::available_parallelism().map_or(16, |n| n.get().clamp(8, 64)));

#[inline(always)]
pub(crate) fn is_multithreaded() -> bool {
    Handle::try_current().is_ok_and(|h| h.runtime_flavor() == RuntimeFlavor::MultiThread)
}

pub(crate) async fn rayon_batch<T, R, F>(items: Vec<T>, f: F) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> R + Send + Sync + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    rayon::spawn(move || {
        let _ = tx.send(items.into_par_iter().map(f).collect());
    });
    rx.await.expect("rayon batch panicked")
}
