use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Cooperative cancellation flag that can also be waited on with a timeout.
#[derive(Clone, Default)]
pub struct CancelToken {
    inner: Arc<(Mutex<bool>, Condvar)>,
}

impl CancelToken {
    pub fn new() -> CancelToken {
        CancelToken::default()
    }

    pub fn cancel(&self) {
        let (m, cv) = &*self.inner;
        *m.lock().unwrap_or_else(|p| p.into_inner()) = true;
        cv.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        *self.inner.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Blocks up to `timeout`; returns true if cancelled.
    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        let (m, cv) = &*self.inner;
        let deadline = Instant::now() + timeout;
        let mut g = m.lock().unwrap_or_else(|p| p.into_inner());
        while !*g {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            g = cv
                .wait_timeout(g, deadline - now)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        *g
    }

    /// A token cancelled by Ctrl-C (SIGINT). The process-wide handler is
    /// installed once and fires the token of the latest call, so a command that
    /// captures twice (Doctor, then its follow-up) can still be interrupted.
    pub fn install_ctrlc() -> CancelToken {
        let token = CancelToken::new();
        *current().lock().unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
        INSTALL.call_once(|| {
            let _ = ctrlc::set_handler(fire_installed);
        });
        token
    }
}

static INSTALL: std::sync::Once = std::sync::Once::new();
static CURRENT: Mutex<Option<CancelToken>> = Mutex::new(None);

fn current() -> &'static Mutex<Option<CancelToken>> {
    &CURRENT
}

/// What the Ctrl-C handler does: cancel the most recently installed token.
pub fn fire_installed() {
    if let Some(t) = current().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        t.cancel();
    }
}
