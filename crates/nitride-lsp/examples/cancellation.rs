//! Gated signal-to-return measurements, including the next polling batch and cleanup.
use nitride_codegen::RuntimeSnapshot;
use nitride_core::{CancellationToken, Limits, RecoveryOptions, SnapshotMode, with_cancellation};
use nitride_lsp::registry::Registry;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "usage: cancellation WORKSPACE SOURCE");
    let path = PathBuf::from(&args[1]);
    let source: Arc<str> = std::fs::read_to_string(&path).unwrap().into();
    let mut registry = Registry::default();
    registry.reload(&[PathBuf::from(&args[0])], &|_| None);
    let owner = registry
        .for_path(&path)
        .unwrap()
        .current
        .as_ref()
        .unwrap()
        .language
        .clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let token = CancellationToken::from_predicate(move || {
        observed.fetch_add(1, Ordering::Relaxed);
        false
    });
    let started = Instant::now();
    let parsed = with_cancellation(&token, || {
        RuntimeSnapshot::parse(
            owner.clone(),
            &source,
            Limits::default(),
            SnapshotMode::Recovering(RecoveryOptions::default()),
        )
    })
    .unwrap();
    let calibration_ns = started.elapsed().as_nanos();
    let calibration_polls = calls.load(Ordering::Relaxed);
    assert!(calibration_polls > 20);
    drop(parsed);
    let mut samples = Vec::new();
    for percent in [10, 50, 90] {
        for _ in 0..3 {
            let target = calibration_polls * percent / 100;
            let cancelled = Arc::new(AtomicBool::new(false));
            let flag = cancelled.clone();
            let count = Arc::new(AtomicUsize::new(0));
            let observed = count.clone();
            let (entered, start) = mpsc::sync_channel(1);
            let (release, gate) = mpsc::sync_channel(1);
            let gate = Mutex::new(gate);
            let token = CancellationToken::from_predicate(move || {
                let at = observed.fetch_add(1, Ordering::Relaxed) + 1;
                if at == target {
                    entered.send(()).unwrap();
                    gate.lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(30))
                        .unwrap();
                    // Deliberately finish this poll uncancelled: the timed signal
                    // must then traverse the NEXT polling batch before aborting.
                    return false;
                }
                flag.load(Ordering::Relaxed)
            });
            let owner = owner.clone();
            let source = source.clone();
            let worker = std::thread::spawn(move || {
                with_cancellation(&token, || {
                    RuntimeSnapshot::parse(
                        owner,
                        &source,
                        Limits::default(),
                        SnapshotMode::Recovering(RecoveryOptions::default()),
                    )
                })
            });
            let reached = start.recv_timeout(Duration::from_secs(60));
            let signalled = Instant::now();
            cancelled.store(true, Ordering::Relaxed);
            let _ = release.send(());
            let result = worker.join().unwrap();
            let latency_ns = signalled.elapsed().as_nanos();
            reached.expect("calibrated checkpoint must be reached");
            let error = result.err().expect("no partial snapshot may return");
            assert_eq!(error.category, "cancelled");
            let polls = count.load(Ordering::Relaxed);
            assert!(polls > target);
            samples.push(json!({"gate_percent_of_calibrated_polls":percent,
                "gate_poll":target,"observed_polls":polls,"signal_to_return_ns":latency_ns,
                "outcome":error.category,"stop_span":error.span}));
        }
    }
    println!(
        "{}",
        json!({"source_bytes":source.len(),"calibration_ns":calibration_ns,
        "calibration_polls":calibration_polls,"samples":samples,
        "metric":"Signal precedes release of an intentionally false gated poll; latency includes next polling batch, unwind/local cleanup and thread join. Gate percentages describe checkpoint ordinals, not wall-time progress or parser phase. Calibration uses a counting false predicate, not a normal-overhead measurement. No universal deadline or GUI claim."})
    );
}
