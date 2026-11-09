//! Owned, bounded document work. Only the event loop commits its completions.
use crate::{
    analysis::{self, Analysis},
    registry::Registry,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub document: u64,
    pub workspace: u64,
}
pub(crate) struct Job {
    pub ticket: u64,
    pub uri: String,
    pub stamp: Stamp,
    pub path: PathBuf,
    pub text: String,
    pub registry: Registry,
    pub buffers: HashMap<PathBuf, String>,
    pub previous: Option<Arc<Analysis>>,
    pub cancelled: Arc<AtomicBool>,
}
impl Job {
    pub fn analyze(&self) -> Option<Analysis> {
        let token = nitride_core::CancellationToken::from_flag(self.cancelled.clone());
        self.analyze_with_cancellation(&token)
    }
    fn analyze_with_cancellation(
        &self,
        token: &nitride_core::CancellationToken,
    ) -> Option<Analysis> {
        nitride_core::with_cancellation(token, || {
            if nitride_core::cancellation_requested() {
                return None;
            }
            let result = analysis::analyze_incremental(
                &self.registry,
                &self.path,
                &self.text,
                &|p| self.buffers.get(&crate::server::canonical(p)).cloned(),
                self.previous.as_deref(),
            );
            if nitride_core::cancellation_requested() {
                None
            } else {
                result
            }
        })
    }
}
pub(crate) enum Outcome {
    Analyzed(Option<Box<Analysis>>),
    Cancelled,
    Failed,
}
pub(crate) struct Completion {
    pub ticket: u64,
    pub uri: String,
    pub stamp: Stamp,
    pub outcome: Outcome,
}
pub(crate) type Analyzer = Box<dyn FnMut(&Job) -> Option<Analysis> + Send>;
pub(crate) struct Worker {
    sender: mpsc::SyncSender<Box<Job>>,
}
impl Worker {
    pub fn spawn(complete: impl Fn(Completion) + Send + 'static, mut analyze: Analyzer) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Box<Job>>(1);
        std::thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                let outcome = if job.cancelled.load(Ordering::Relaxed) {
                    Outcome::Cancelled
                } else {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| analyze(&job))) {
                        Ok(_) if job.cancelled.load(Ordering::Relaxed) => Outcome::Cancelled,
                        Ok(result) => Outcome::Analyzed(result.map(Box::new)),
                        Err(_) => Outcome::Failed,
                    }
                };
                complete(Completion {
                    ticket: job.ticket,
                    uri: job.uri.clone(),
                    stamp: job.stamp,
                    outcome,
                });
            }
        });
        Self { sender }
    }
    pub fn submit(&self, job: Job) -> bool {
        self.sender.try_send(Box::new(job)).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;

    fn job(ticket: u64) -> Job {
        Job {
            ticket,
            uri: "file:///document.txt".into(),
            stamp: Stamp {
                document: ticket,
                workspace: 0,
            },
            path: "document.txt".into(),
            text: format!("version-{ticket}"),
            registry: Registry::default(),
            buffers: HashMap::new(),
            previous: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
    fn completion(receiver: &mpsc::Receiver<Completion>) -> Completion {
        receiver.recv_timeout(Duration::from_secs(3)).unwrap()
    }

    fn served_job(ticket: u64) -> Job {
        static REGISTRY: std::sync::OnceLock<std::sync::Mutex<Registry>> =
            std::sync::OnceLock::new();
        let registry = REGISTRY
            .get_or_init(|| {
                let mut registry = Registry::default();
                registry.reload(
                    &[Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")],
                    &|_| None,
                );
                std::sync::Mutex::new(registry)
            })
            .lock()
            .unwrap()
            .clone();
        let mut job = job(ticket);
        job.registry = registry;
        job.path = "sample.csharp4".into();
        job.text = "class Current {}".into();
        job
    }

    #[test]
    fn job_analysis_scopes_its_existing_obsolete_flag() {
        let job = served_job(1);
        job.cancelled.store(true, Ordering::Relaxed);
        assert!(
            job.analyze().is_none(),
            "cancelled job must not analyze a served document"
        );
    }

    #[test]
    fn real_parser_releases_worker_after_cancellation_then_runs_latest_job() {
        use std::sync::atomic::AtomicUsize;
        let (send, receive) = mpsc::channel();
        let (entered, start) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let gate = std::sync::Mutex::new(gate);
        let mut obsolete = served_job(1);
        obsolete.text = "class Obsolete { int field; }\n".repeat(10_000);
        let flag = obsolete.cancelled.clone();
        let observed_flag = flag.clone();
        let calls = AtomicUsize::new(0);
        let token = nitride_core::CancellationToken::from_predicate(move || {
            if calls.fetch_add(1, Ordering::Relaxed) == 8 {
                entered.send(()).unwrap();
                gate.lock().unwrap().recv().unwrap();
            }
            observed_flag.load(Ordering::Relaxed)
        });
        let worker = Worker::spawn(
            move |done| {
                send.send(done).unwrap();
            },
            Box::new(move |job| {
                if job.ticket == 1 {
                    job.analyze_with_cancellation(&token)
                } else {
                    job.analyze()
                }
            }),
        );
        assert!(worker.submit(obsolete));
        start
            .recv_timeout(Duration::from_secs(5))
            .expect("actual parser checkpoints reached");
        flag.store(true, Ordering::Relaxed);
        release.send(()).unwrap();
        assert!(matches!(completion(&receive).outcome, Outcome::Cancelled));
        assert!(worker.submit(served_job(2)));
        match completion(&receive).outcome {
            Outcome::Analyzed(Some(analysis)) => {
                assert_eq!(analysis.lines.text(), "class Current {}");
                assert!(analysis.problems.is_empty());
            }
            _ => panic!("latest job must succeed after cancellation"),
        }
    }
    #[test]
    fn owned_job_executes_on_worker_and_returns_its_identity() {
        let parent = std::thread::current().id();
        let (send, receive) = mpsc::channel();
        let worker = Worker::spawn(
            move |done| {
                send.send(done).unwrap();
            },
            Box::new(move |job| {
                assert_ne!(std::thread::current().id(), parent);
                assert_eq!(job.text, "version-7");
                job.analyze()
            }),
        );
        assert!(worker.submit(job(7)));
        let done = completion(&receive);
        assert_eq!(done.ticket, 7);
        assert_eq!(done.stamp.document, 7);
        assert_eq!(done.uri, "file:///document.txt");
        assert!(matches!(done.outcome, Outcome::Analyzed(None)));
    }
    #[test]
    fn obsolete_job_skips_analysis() {
        let (send, receive) = mpsc::channel();
        let worker = Worker::spawn(
            move |done| {
                send.send(done).unwrap();
            },
            Box::new(|_| panic!("cancelled job must not run")),
        );
        let job = job(1);
        job.cancelled.store(true, Ordering::Relaxed);
        assert!(worker.submit(job));
        assert!(matches!(completion(&receive).outcome, Outcome::Cancelled));
    }
    #[test]
    fn cancellation_during_analysis_discards_the_result() {
        let (send, receive) = mpsc::channel();
        let (started, start) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let worker = Worker::spawn(
            move |done| {
                send.send(done).unwrap();
            },
            Box::new(move |_| {
                started.send(()).unwrap();
                gate.recv().unwrap();
                None
            }),
        );
        let job = job(1);
        let cancelled = job.cancelled.clone();
        assert!(worker.submit(job));
        start.recv_timeout(Duration::from_secs(3)).unwrap();
        cancelled.store(true, Ordering::Relaxed);
        release.send(()).unwrap();
        assert!(matches!(completion(&receive).outcome, Outcome::Cancelled));
    }
    #[test]
    fn previous_analysis_is_retained_without_copying_and_then_released() {
        let analysis = Arc::new(
            analysis::analyze(
                &Registry::default(),
                Path::new("memory.nproj"),
                "workspace {}",
                &|_| None,
            )
            .unwrap(),
        );
        let weak = Arc::downgrade(&analysis);
        let (send, receive) = mpsc::channel();
        let (started, start) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let worker = Worker::spawn(
            move |done| {
                send.send(done).unwrap();
            },
            Box::new(move |job| {
                if job.ticket == 1 {
                    assert!(job.previous.is_some());
                    started.send(()).unwrap();
                    gate.recv().unwrap();
                }
                None
            }),
        );
        let mut first = job(1);
        first.previous = Some(analysis.clone());
        drop(analysis);
        assert!(worker.submit(first));
        start.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(weak.upgrade().is_some());
        release.send(()).unwrap();
        completion(&receive);
        assert!(worker.submit(job(2)));
        completion(&receive);
        assert!(weak.upgrade().is_none());
    }
    #[test]
    #[cfg(panic = "unwind")]
    fn unwinding_panic_completes_and_worker_accepts_next_job() {
        let (send, receive) = mpsc::channel();
        let worker = Worker::spawn(
            move |done| {
                send.send(done).unwrap();
            },
            Box::new(|job| {
                if job.ticket == 1 {
                    panic!("worker fixture");
                }
                None
            }),
        );
        assert!(worker.submit(job(1)));
        assert!(matches!(completion(&receive).outcome, Outcome::Failed));
        assert!(worker.submit(job(2)));
        assert!(matches!(
            completion(&receive).outcome,
            Outcome::Analyzed(None)
        ));
    }
}
