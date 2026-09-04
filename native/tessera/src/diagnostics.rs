use crate::util::{FastHashMap, FastHashSet};
use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt::Display;
use std::fmt::Write;
use std::panic::Location;
use std::sync::Mutex;

type Site = &'static Location<'static>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub severity: Severity,
    pub subject: String,
    pub message: Cow<'static, str>,
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{level}: {}: {}", self.subject, self.message)
    }
}

const MAX_RECORDED: usize = 1024;

#[derive(Default)]
struct Inner {
    /// for dedup
    seen: FastHashMap<(Severity, Site), FastHashSet<String>>,
    ordered: Vec<Diagnostic>,
    suppressed: usize,
}

#[derive(Default)]
pub struct Diagnostics {
    inner: Mutex<Inner>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Degraded but rendered
    #[track_caller]
    pub fn warn<M: Into<Cow<'static, str>>>(&self, subject: impl Display, message: impl FnOnce() -> M) {
        self.record(Severity::Warning, Location::caller(), subject, "", message);
    }

    /// Degraded but rendered
    #[track_caller]
    pub fn warn_keyed<M: Into<Cow<'static, str>>>(
        &self,
        subject: impl Display,
        key: impl Display,
        message: impl FnOnce() -> M,
    ) {
        self.record(Severity::Warning, Location::caller(), subject, key, message);
    }

    /// Substituted or skipped
    #[track_caller]
    pub fn error<M: Into<Cow<'static, str>>>(&self, subject: impl Display, message: impl FnOnce() -> M) {
        self.record(Severity::Error, Location::caller(), subject, "", message);
    }

    fn record<M: Into<Cow<'static, str>>>(
        &self,
        severity: Severity,
        site: Site,
        subject: impl Display,
        key: impl Display,
        message: impl FnOnce() -> M,
    ) {
        thread_local! {
            static SUBJECT: RefCell<String> = const { RefCell::new(String::new()) };
        }
        SUBJECT.with_borrow_mut(|buf| {
            buf.clear();
            let _ = write!(buf, "{subject}");
            let split = buf.len();
            let _ = write!(buf, "\u{0}{key}");

            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if inner.seen.get(&(severity, site)).is_some_and(|s| s.contains(buf.as_str())) {
                return;
            }
            if inner.ordered.len() >= MAX_RECORDED {
                inner.suppressed += 1;
                return;
            }
            inner.ordered.push(Diagnostic {
                severity,
                subject: buf[..split].to_string(),
                message: message().into(),
            });
            inner.seen.entry((severity, site)).or_default().insert(buf.clone());
        });
    }

    pub fn snapshot(&self) -> Vec<Diagnostic> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.ordered.clone()
    }

    pub fn drain(&self) -> Vec<Diagnostic> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.seen.clear();
        inner.suppressed = 0;
        std::mem::take(&mut inner.ordered)
    }

    pub fn suppressed(&self) -> usize {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.suppressed
    }

    pub fn is_empty(&self) -> bool {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.ordered.is_empty()
    }
}
