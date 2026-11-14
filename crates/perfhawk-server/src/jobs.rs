//! One-slot capture job manager: start, poll, cancel. In memory only.

use perfhawk_capture::CancelToken;
use serde::Serialize;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Progress lines kept per job; older ones are dropped.
pub const MAX_PROGRESS_LINES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobSnapshot {
    pub job_id: String,
    pub state: JobState,
    pub started_ms: i64,
    pub duration_ms: i64,
    pub progress: Vec<String>,
    pub session_id: Option<String>,
    pub error: Option<JobError>,
}

/// A capture is already running.
#[derive(Debug)]
pub struct JobBusy;

struct Inner {
    snap: JobSnapshot,
    started: Instant,
}

struct Job {
    shared: Arc<Mutex<Inner>>,
    cancel: CancelToken,
}

#[derive(Default)]
pub struct JobManager {
    current: Mutex<Option<Job>>,
    counter: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl JobManager {
    /// Runs `work` on its own thread. `work` returns the finished session's id.
    /// A job whose token was cancelled before `work` returned `Ok` ends `Cancelled`.
    pub fn start<F>(&self, work: F) -> Result<JobSnapshot, JobBusy>
    where
        F: FnOnce(&CancelToken, &(dyn Fn(&str) + Sync)) -> Result<String, JobError>
            + Send
            + 'static,
    {
        let mut slot = lock(&self.current);
        if let Some(j) = slot.as_ref() {
            if lock(&j.shared).snap.state == JobState::Running {
                return Err(JobBusy);
            }
        }
        let started_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let snap = JobSnapshot {
            job_id: format!("job-{started_ms}-{n}"),
            state: JobState::Running,
            started_ms,
            duration_ms: 0,
            progress: Vec::new(),
            session_id: None,
            error: None,
        };
        let first = snap.clone();
        let shared = Arc::new(Mutex::new(Inner {
            snap,
            started: Instant::now(),
        }));
        let cancel = CancelToken::new();
        let (sh, tok) = (Arc::clone(&shared), cancel.clone());
        let spawned = std::thread::Builder::new()
            .name("perfhawk-capture".into())
            .spawn(move || {
                let push = |line: &str| {
                    let mut g = lock(&sh);
                    if g.snap.progress.len() >= MAX_PROGRESS_LINES {
                        g.snap.progress.remove(0);
                    }
                    g.snap.progress.push(line.to_string());
                };
                let res = std::panic::catch_unwind(AssertUnwindSafe(|| work(&tok, &push)))
                    .unwrap_or_else(|_| {
                        Err(JobError {
                            code: "internal".into(),
                            message: "the capture stopped unexpectedly".into(),
                        })
                    });
                let mut g = lock(&sh);
                g.snap.duration_ms = g.started.elapsed().as_millis() as i64;
                match res {
                    Ok(id) => {
                        g.snap.state = if tok.is_cancelled() {
                            JobState::Cancelled
                        } else {
                            JobState::Done
                        };
                        g.snap.session_id = Some(id);
                    }
                    Err(e) => {
                        g.snap.state = if tok.is_cancelled() {
                            JobState::Cancelled
                        } else {
                            JobState::Failed
                        };
                        g.snap.error = Some(e);
                    }
                }
            });
        if spawned.is_err() {
            let mut g = lock(&shared);
            g.snap.state = JobState::Failed;
            g.snap.error = Some(JobError {
                code: "internal".into(),
                message: "could not start the capture thread".into(),
            });
        }
        *slot = Some(Job { shared, cancel });
        Ok(first)
    }

    /// The running or most recent job.
    pub fn snapshot(&self) -> Option<JobSnapshot> {
        let slot = lock(&self.current);
        let j = slot.as_ref()?;
        let g = lock(&j.shared);
        let mut s = g.snap.clone();
        if s.state == JobState::Running {
            s.duration_ms = g.started.elapsed().as_millis() as i64;
        }
        Some(s)
    }

    /// Fires the job's cancel token. False if `job_id` is not the current job.
    /// Cancelling a finished job does nothing.
    pub fn cancel(&self, job_id: &str) -> bool {
        let slot = lock(&self.current);
        match slot.as_ref() {
            Some(j) if lock(&j.shared).snap.job_id == job_id => {
                if lock(&j.shared).snap.state == JobState::Running {
                    j.cancel.cancel();
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_not_running(m: &JobManager) -> JobSnapshot {
        let t = Instant::now();
        loop {
            let s = m.snapshot().expect("a job exists");
            if s.state != JobState::Running {
                return s;
            }
            assert!(t.elapsed() < Duration::from_secs(10), "job never finished");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn no_snapshot_before_any_job() {
        assert!(JobManager::default().snapshot().is_none());
    }

    #[test]
    fn progress_is_bounded_and_keeps_newest() {
        let m = JobManager::default();
        m.start(|_c, progress| {
            for i in 0..500 {
                progress(&format!("line {i}"));
            }
            Ok("abc".to_string())
        })
        .unwrap();
        let s = wait_not_running(&m);
        assert_eq!(s.state, JobState::Done);
        assert_eq!(s.session_id.as_deref(), Some("abc"));
        assert_eq!(s.progress.len(), MAX_PROGRESS_LINES);
        assert_eq!(s.progress.last().unwrap(), "line 499");
    }

    #[test]
    fn second_start_while_running_is_busy_and_first_is_unaffected() {
        let m = JobManager::default();
        let first = m
            .start(|c, _p| {
                c.wait_timeout(Duration::from_secs(30));
                Ok("x".to_string())
            })
            .unwrap();
        assert!(m.start(|_c, _p| Ok("y".to_string())).is_err());
        let s = m.snapshot().unwrap();
        assert_eq!(
            (s.job_id.as_str(), s.state),
            (first.job_id.as_str(), JobState::Running)
        );
        assert!(m.cancel(&first.job_id));
        assert_eq!(wait_not_running(&m).state, JobState::Cancelled);
    }

    #[test]
    fn cancel_unknown_id_is_false_and_after_done_is_a_noop() {
        let m = JobManager::default();
        let j = m.start(|_c, _p| Ok("s".to_string())).unwrap();
        wait_not_running(&m);
        assert!(!m.cancel("nope"));
        assert!(m.cancel(&j.job_id));
        assert_eq!(m.snapshot().unwrap().state, JobState::Done);
    }

    #[test]
    fn a_failing_job_reports_its_error_and_frees_the_slot() {
        let m = JobManager::default();
        m.start(|_c, _p| {
            Err(JobError {
                code: "needs_privilege".into(),
                message: "run elevated".into(),
            })
        })
        .unwrap();
        let s = wait_not_running(&m);
        assert_eq!(s.state, JobState::Failed);
        assert_eq!(s.error.unwrap().code, "needs_privilege");
        assert!(m.start(|_c, _p| Ok("ok".to_string())).is_ok());
    }
}
