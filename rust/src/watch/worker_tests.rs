use super::*;
use std::sync::Arc;

#[tokio::test]
async fn shutdown_cancels_and_joins_blocking_worker_before_returning() {
    let cancellation = Cancellation::default();
    let worker_cancellation = cancellation.clone();
    let task = tokio::task::spawn_blocking(move || {
        while worker_cancellation.check().is_ok() {
            std::thread::yield_now();
        }
        assert!(worker_cancellation
            .assemble(|| -> Result<(), String> { panic!("assembly started after shutdown") })
            .is_err());
        42
    });
    tokio::time::timeout(
        Duration::from_secs(5),
        cancel_and_join(&cancellation, Some(task)),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(cancellation.check().is_err());
}

#[tokio::test]
async fn shutdown_drains_assembly_already_in_progress() {
    let cancellation = Cancellation::default();
    let worker_cancellation = cancellation.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let committed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_committed = committed.clone();
    let task = tokio::task::spawn_blocking(move || {
        worker_cancellation
            .assemble(|| {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                worker_committed.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
    });
    started_rx.await.unwrap();
    let shutdown = cancel_and_join(&cancellation, Some(task));
    tokio::pin!(shutdown);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut shutdown)
            .await
            .is_err()
    );
    assert!(cancellation.check().is_err());
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), shutdown)
        .await
        .unwrap()
        .unwrap();
    assert!(committed.load(std::sync::atomic::Ordering::SeqCst));
}

#[test]
fn debounce_is_trailing_edge() {
    let now = Instant::now();
    let mut scheduler = Scheduler::default();
    scheduler.changed(now);
    scheduler.changed(now + Duration::from_millis(150));
    assert!(!scheduler.ready(now + Duration::from_millis(349)));
    assert!(scheduler.ready(now + Duration::from_millis(350)));
}

#[test]
fn changes_during_build_produce_only_one_trailing_build() {
    let now = Instant::now();
    let mut scheduler = Scheduler::default();
    scheduler.start();
    for offset in 0..100 {
        scheduler.changed(now + Duration::from_millis(offset));
    }
    assert!(!scheduler.ready(now + Duration::from_secs(1)));
    scheduler.finished();
    assert!(scheduler.ready(now + Duration::from_secs(1)));
    scheduler.start();
    scheduler.finished();
    assert!(!scheduler.ready(now + Duration::from_secs(2)));
}

#[test]
fn trailing_build_waits_for_latest_event() {
    let now = Instant::now();
    let mut scheduler = Scheduler::default();
    scheduler.start();
    scheduler.changed(now);
    scheduler.finished();
    assert!(!scheduler.ready(now + Duration::from_millis(199)));
    assert!(scheduler.ready(now + DEBOUNCE));
}
