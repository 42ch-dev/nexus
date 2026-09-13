//! P2-T3 LocalSet shutdown integration: tracked tasks, bounds, control-channel bypass.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use nexus_acp_host::localset_bridge::{
    LocalSetBridge, MAX_ACTIVE_TASKS, MAX_PENDING_BYTES, MAX_PENDING_REQUESTS, SHUTDOWN_JOIN_BUDGET,
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
        assert_eq!(
            started.load(Ordering::SeqCst),
            1,
            "task must start before cancel"
        );
        pending.abort();
        let _ = pending.await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            stopped.load(Ordering::SeqCst),
            0,
            "cancelled task must not continue"
        );
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
            .execute(1, || {
                Box::pin(async { tokio::time::sleep(Duration::from_millis(20)).await })
            })
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
        let saturate_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < saturate_deadline {
            if bridge.stats().active_tasks >= MAX_ACTIVE_TASKS {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(bridge.stats().active_tasks, MAX_ACTIVE_TASKS);
        let b = bridge.clone();
        let deferred = tokio::spawn(async move {
            let _ = b
                .execute(1, || Box::pin(async { std::future::pending::<()>().await }))
                .await;
        });
        let defer_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < defer_deadline {
            if bridge.stats().pending_requests > 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(
            bridge.stats().pending_requests > 0,
            "extra work must defer when active cap is saturated"
        );
        deferred.abort();
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
        let err = bridge.execute(oversize, || Box::pin(async { 1 })).await;
        assert!(
            err.is_err(),
            "oversize payload must be rejected at admission"
        );
        let _ = bridge.shutdown().await;
    }
    {
        let bridge = LocalSetBridge::new();
        let multibyte = "🎉".repeat((MAX_PENDING_BYTES / 4) + 1);
        let err = bridge
            .execute(multibyte.len(), move || {
                Box::pin(async move { multibyte.len() })
            })
            .await;
        assert!(err.is_err(), "utf-8 multibyte oversize must fail admission");
        let _ = bridge.shutdown().await;
    }

    {
        let bridge = LocalSetBridge::new();
        let hang_id = bridge.stats().active_tasks;
        let b = bridge.clone();
        let hang = tokio::spawn(async move {
            let _ = b
                .execute(8, || Box::pin(async { std::future::pending::<()>().await }))
                .await;
        });
        tokio::time::sleep(Duration::from_millis(40)).await;
        let evidence = bridge.shutdown().await;
        hang.abort();
        assert!(
            !evidence.aborted_task_ids.is_empty(),
            "shutdown must record aborted live task ids: {evidence:?}"
        );
        assert_eq!(evidence.active_tasks, 0);
        let _ = hang_id;
    }

    {
        let bridge = LocalSetBridge::new();
        let bridge_ref = bridge.clone();
        let mut handles = Vec::new();
        for i in 0..MAX_ACTIVE_TASKS {
            let b = bridge_ref.clone();
            handles.push(tokio::spawn(async move {
                let _ = b
                    .execute(8, move || {
                        Box::pin(async move {
                            tokio::time::sleep(Duration::from_secs(30)).await;
                            i
                        })
                    })
                    .await;
            }));
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(bridge.stats().active_tasks, MAX_ACTIVE_TASKS);
        let b = bridge.clone();
        handles.push(tokio::spawn(async move {
            let _ = b.execute(8, move || Box::pin(async { 99 })).await;
        }));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            bridge.stats().active_tasks,
            MAX_ACTIVE_TASKS,
            "deferred must not exceed cap"
        );
        for h in handles {
            h.abort();
        }
        let _ = bridge.shutdown().await;
    }

    {
        // Cancel after TaskSlot install but before first poll: active slot released once.
        let bridge = LocalSetBridge::new();
        let b = bridge.clone();
        let exec = tokio::spawn(async move {
            let _ = b
                .execute(8, || Box::pin(async { std::future::pending::<()>().await }))
                .await;
        });
        bridge.wait_task_slot_installed().await;
        assert_eq!(
            bridge.stats().active_tasks,
            1,
            "TaskSlot must reserve active slot"
        );
        bridge.cancel_task(1).await.expect("cancel task 1");
        let _ = exec.await;
        assert_eq!(
            bridge.stats().active_tasks,
            0,
            "pre-poll cancel must release active reservation exactly once"
        );
        let evidence = bridge.shutdown().await;
        assert_eq!(evidence.active_tasks, 0, "shutdown evidence: {evidence:?}");
        // 32 admissions must succeed after the slot is released.
        let mut handles = Vec::new();
        for _ in 0..MAX_ACTIVE_TASKS {
            let br = bridge.clone();
            handles.push(tokio::spawn(async move {
                let _ = br.execute(1, || Box::pin(async { 1 })).await;
            }));
        }
        for h in handles {
            let _ = h.await;
        }
        assert_eq!(bridge.stats().active_tasks, 0);
    }

    {
        // Cancellation/teardown work bypasses a saturated work queue, and a
        // draining bridge refuses new work while the close snapshot reports what
        // it drained.
        let bridge = LocalSetBridge::new();
        let bridge_ref = bridge.clone();
        let mut blockers = Vec::new();
        for _ in 0..MAX_ACTIVE_TASKS {
            let b = bridge_ref.clone();
            blockers.push(tokio::spawn(async move {
                let _ = b
                    .execute(4, || Box::pin(async { std::future::pending::<()>().await }))
                    .await;
            }));
        }
        let saturate = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < saturate {
            if bridge.stats().active_tasks >= MAX_ACTIVE_TASKS {
                break;
            }
            tokio::task::yield_now().await;
        }
        for _ in 0..MAX_PENDING_REQUESTS {
            let b = bridge_ref.clone();
            blockers.push(tokio::spawn(async move {
                let _ = b
                    .execute(4, || Box::pin(async { std::future::pending::<()>().await }))
                    .await;
            }));
        }
        let queued = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < queued {
            if bridge.stats().pending_requests >= MAX_PENDING_REQUESTS {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(bridge.stats().pending_requests, MAX_PENDING_REQUESTS);

        // The queue is full; control work must still run.
        let control_ran = Arc::new(AtomicUsize::new(0));
        let control_flag = control_ran.clone();
        bridge
            .execute_control(4, move || {
                let flag = control_flag;
                Box::pin(async move {
                    flag.fetch_add(1, Ordering::SeqCst);
                })
            })
            .await
            .expect("control work must not be blocked by a full work queue");
        assert_eq!(control_ran.load(Ordering::SeqCst), 1);

        bridge.begin_drain();
        let refused = bridge.execute(4, || Box::pin(async { 1 })).await;
        assert!(refused.is_err(), "a draining bridge must refuse new work");

        let evidence = bridge.shutdown().await;
        assert_eq!(
            evidence.queued_at_shutdown, MAX_PENDING_REQUESTS,
            "close snapshot must report the queued requests it drained: {evidence:?}"
        );
        assert!(
            evidence.queued_bytes_at_shutdown > 0,
            "close snapshot must report charged bytes: {evidence:?}"
        );
        assert_eq!(evidence.active_at_shutdown, MAX_ACTIVE_TASKS);
        assert_eq!(evidence.pending_requests, 0, "queue must be drained");
        assert_eq!(evidence.active_tasks, 0);
        assert_eq!(evidence.control_tasks, 0);
        assert!(
            !evidence.aborted_task_ids.is_empty(),
            "aborted work and ownership tasks must be reported: {evidence:?}"
        );
        assert!(evidence.joined_cleanly || !evidence.thread_alive);
        for h in blockers {
            h.abort();
        }
    }

    {
        // Active cap is independent of pending queue (pending16 vs active32).
        let bridge = LocalSetBridge::new();
        let bridge_ref = bridge.clone();
        let mut blockers = Vec::new();
        for _ in 0..MAX_ACTIVE_TASKS {
            let b = bridge_ref.clone();
            blockers.push(tokio::spawn(async move {
                let _ = b
                    .execute(4, || Box::pin(async { std::future::pending::<()>().await }))
                    .await;
            }));
        }
        let saturate_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < saturate_deadline {
            if bridge.stats().active_tasks >= MAX_ACTIVE_TASKS {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            bridge.stats().active_tasks,
            MAX_ACTIVE_TASKS,
            "must saturate active cap before pending admission proof"
        );
        for _ in 0..MAX_PENDING_REQUESTS {
            let b = bridge_ref.clone();
            blockers.push(tokio::spawn(async move {
                let _ = b
                    .execute(4, || Box::pin(async { std::future::pending::<()>().await }))
                    .await;
            }));
        }
        let pending_deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < pending_deadline {
            if bridge.stats().pending_requests >= MAX_PENDING_REQUESTS {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            bridge.stats().pending_requests,
            MAX_PENDING_REQUESTS,
            "pending queue must fill independently of active cap"
        );
        assert_eq!(
            bridge.stats().active_tasks,
            MAX_ACTIVE_TASKS,
            "active cap must stay saturated while pending queue fills"
        );
        for h in blockers {
            h.abort();
        }
        let _ = bridge.shutdown().await;
    }
}
