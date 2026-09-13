//! P2-T3 LocalSet shutdown integration: tracked tasks, bounds, control-channel bypass.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use nexus_acp_host::localset_bridge::{
    LocalSetBridge, MAX_ACTIVE_TASKS, MAX_PENDING_BYTES, MAX_PENDING_REQUESTS,
    SHUTDOWN_JOIN_BUDGET,
};

#[tokio::test]
async fn localset_shutdown() {
    for cycle in 0..100 {
        let bridge = LocalSetBridge::new();
        let payload = format!("cycle-{cycle}");
        let byte_charge = payload.len();
        let n: i32 = bridge
            .execute(byte_charge, move || Box::pin(async move { cycle }))
            .await
            .expect("execute in cycle");
        assert_eq!(n, cycle);
        let evidence = bridge.shutdown().await;
        assert!(
            evidence.joined_cleanly || !evidence.thread_alive,
            "cycle {cycle}: thread must join, evidence={evidence:?}"
        );
        assert_eq!(evidence.active_tasks, 0, "cycle {cycle}");
        assert_eq!(evidence.pending_requests, 0, "cycle {cycle}");
    }

    {
        let bridge = LocalSetBridge::new();
        let started = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicUsize::new(0));
        let started_clone = started.clone();
        let stopped_clone = stopped.clone();
        let b = bridge.clone();
        let pending = tokio::spawn(async move {
            let result = b
                .execute(8, move || {
                    let started = started_clone;
                    let stopped = stopped_clone;
                    Box::pin(async move {
                        started.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        stopped.fetch_add(1, Ordering::SeqCst);
                        1
                    })
                })
                .await;
            assert!(result.is_err(), "cancelled waiter must not succeed");
        });
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(started.load(Ordering::SeqCst), 1, "task must start before cancel");
        pending.abort();
        let _ = pending.await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(stopped.load(Ordering::SeqCst), 0, "cancelled task must not continue");
        assert_eq!(bridge.stats().active_tasks, 0);
        let evidence = bridge.shutdown().await;
        assert!(evidence.joined_cleanly || !evidence.thread_alive);
    }

    {
        let bridge = LocalSetBridge::new();
        let bridge_ref = bridge.clone();
        let blockers = (0..MAX_PENDING_REQUESTS)
            .map(|i| {
                let b = bridge_ref.clone();
                let charge = 4 + i;
                tokio::spawn(async move {
                    let _ = b
                        .execute(charge, || {
                            Box::pin(async { tokio::time::sleep(Duration::from_secs(60)).await })
                        })
                        .await;
                })
            })
            .collect::<Vec<_>>();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let shutdown_started = std::time::Instant::now();
        let evidence = bridge.shutdown().await;
        let elapsed = shutdown_started.elapsed();
        assert!(
            elapsed <= SHUTDOWN_JOIN_BUDGET + Duration::from_millis(500),
            "shutdown bypassed full queue in {:?}, evidence={evidence:?}",
            elapsed
        );
        for handle in blockers {
            handle.abort();
        }
    }

    {
        let bridge = LocalSetBridge::new();
        let _ = bridge
            .execute(1, || Box::pin(async { tokio::time::sleep(Duration::from_millis(20)).await }))
            .await;
        let b1 = bridge.clone();
        let b2 = bridge.clone();
        let (e1, e2) = tokio::join!(b1.shutdown(), b2.shutdown());
        assert!(e1.joined_cleanly || !e1.thread_alive);
        assert!(e2.joined_cleanly || !e2.thread_alive);
    }

    {
        let bridge = LocalSetBridge::new();
        let b = bridge.clone();
        let hang = tokio::spawn(async move {
            let _ = b
                .execute(4, || Box::pin(async { std::future::pending::<()>().await }))
                .await;
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        let evidence = bridge.shutdown().await;
        hang.abort();
        assert_eq!(evidence.active_tasks, 0);
        assert!(evidence.joined_cleanly || !evidence.thread_alive);
    }

    {
        let bridge = LocalSetBridge::new();
        let bridge_ref = bridge.clone();
        let mut handles = Vec::new();
        for _ in 0..MAX_ACTIVE_TASKS {
            let b = bridge_ref.clone();
            handles.push(tokio::spawn(async move {
                let _ = b
                    .execute(16, || {
                        Box::pin(async { tokio::time::sleep(Duration::from_secs(5)).await })
                    })
                    .await;
            }));
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(bridge.stats().active_tasks, MAX_ACTIVE_TASKS);
        let busy = bridge.execute(1, || Box::pin(async { 99 })).await;
        assert!(busy.is_err(), "expected busy at active cap");
        for h in handles {
            h.abort();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(bridge.stats().active_tasks, 0);
        let _ = bridge.shutdown().await;
    }

    {
        let bridge = LocalSetBridge::new();
        let oversize = MAX_PENDING_BYTES + 1;
        let err = bridge
            .execute(oversize, || Box::pin(async { 1 }))
            .await;
        assert!(err.is_err(), "oversize payload must be rejected at admission");
        let _ = bridge.shutdown().await;
    }
}
