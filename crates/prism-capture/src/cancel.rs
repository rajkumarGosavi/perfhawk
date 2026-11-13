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

    /// A token cancelled by Ctrl-C (SIGINT). Only the first call in a process
    /// installs the handler; later calls return a token that is never fired.
    pub fn install_ctrlc() -> CancelToken {
        let token = CancelToken::new();
        let t = token.clone();
        let _ = ctrlc::set_handler(move || t.cancel());
        token
    }
}
