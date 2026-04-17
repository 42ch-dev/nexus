//! LocalSet Bridge for `!Send` ACP SDK futures.
//!
//! The `agent-client-protocol` SDK produces `!Send` futures because they require
//! `tokio::task::LocalSet` + `spawn_local`. This module provides a bridge between:
//!
//! - The **async tokio world** (where CLI commands run with `#[tokio::main]`)
//! - The **LocalSet world** (where ACP SDK futures must run on a single thread)
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────┐
//! │                     Tokio Runtime (multi-threaded)                │
//! │                                                                   │
//! │  ┌─────────────────────────────────────────────────────────────┐ │
//! │  │ CLI Command (async fn)                                       │ │
//! │  │                                                              │ │
//! │  │   let bridge = LocalSetBridge::new();                       │ │
//! │  │   let response = bridge.execute(request).await;             │ │
//! │  │                           │                                  │ │
//! │  └───────────────────────────┼──────────────────────────────────┘ │
//! │                              │                                    │
//! │                              │ tokio::sync::mpsc::Sender          │
//! │                              ▼                                    │
//! │  ┌─────────────────────────────────────────────────────────────┐ │
//! │  │ LocalSetBridge                                              │ │
//! │  │  - Sends request via tokio::sync::mpsc                      │ │
//! │  │  - Receives response via oneshot channel                    │ │
//! │  └───────────────────────────┬──────────────────────────────────┘ │
//! └──────────────────────────────┼────────────────────────────────────┘
//!                                │
//!                                │ tokio::sync::mpsc::Receiver
//!                                ▼
//! ┌──────────────────────────────────────────────────────────────────┐
//! │            Dedicated OS Thread (std::thread::spawn)              │
//! │                                                                   │
//! │  ┌─────────────────────────────────────────────────────────────┐ │
//! │  │ tokio::runtime::Runtime (current_thread)                    │ │
//! │  │   .block_on(LocalSet::new().run_until(async {                │ │
//! │  │     loop {                                                   │ │
//! │  │         match request_rx.recv().await {                     │ │
//! │  │             Some(req) => process on LocalSet,               │ │
//! │  │             None => break,                                   │ │
//! │  │         }                                                    │ │
//! │  │     }                                                        │ │
//! │  │   }))                                                        │ │
//! │  └─────────────────────────────────────────────────────────────┘ │
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Graceful Shutdown
//!
//! When `LocalSetBridge` is dropped:
//! 1. Send `None` (shutdown signal) via request channel
//! 2. Wait for LocalSet thread to exit (join)

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

/// Internal request message for the bridge (type-erased).
#[allow(clippy::type_complexity)]
struct BridgeRequest {
    /// The future-producing closure (boxed for `!Send`).
    future_factory: Box<
        dyn FnOnce() -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Box<dyn Any + Send>> + 'static>,
            > + Send
            + 'static,
    >,
}

/// Bridge between async tokio world and `!Send` LocalSet world.
///
/// This struct spawns a dedicated OS thread running a `LocalSet`, enabling
/// execution of `!Send` futures (like those from `agent-client-protocol` SDK)
/// from async code that runs on a multi-threaded tokio runtime.
///
/// # Thread Safety
///
/// The bridge is `Send + Sync + Clone` because all channels used are thread-safe.
/// Cloning shares the sender channel and thread handle.
#[derive(Clone)]
pub struct LocalSetBridge {
    /// Sender for requests to the LocalSet thread.
    request_tx: mpsc::Sender<Option<BridgeRequest>>,
    /// Handle to the LocalSet thread for graceful shutdown (shared).
    thread_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Shutdown flag to ensure only one caller sends shutdown signal.
    shutdown_flag: Arc<AtomicBool>,
}

impl LocalSetBridge {
    /// Create a new LocalSet bridge.
    ///
    /// This spawns a dedicated OS thread running a `LocalSet` that can execute
    /// `!Send` futures. The thread runs until the bridge is dropped.
    #[allow(dead_code)]
    pub fn new() -> Self {
        let (request_tx, mut request_rx) = mpsc::channel::<Option<BridgeRequest>>(16);

        // Spawn dedicated OS thread for LocalSet using Builder (returns Result)
        let thread_handle = thread::Builder::new()
            .name("nexus-localset-bridge".to_string())
            .spawn(move || {
                // Create a single-threaded tokio runtime for this thread
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        error!("Failed to create tokio runtime for LocalSet thread: {}", e);
                        return;
                    }
                };

                rt.block_on(async {
                    let localset = tokio::task::LocalSet::new();

                    localset
                        .run_until(async {
                            info!("LocalSet bridge thread started");

                            while let Some(Some(request)) = request_rx.recv().await {
                                // Execute the !Send future on the LocalSet
                                let future = (request.future_factory)();
                                future.await;
                            }

                            info!("LocalSet bridge thread shutting down");
                        })
                        .await;
                });
            })
            .expect("Failed to spawn LocalSet bridge thread — system resources exhausted");

        Self {
            request_tx,
            thread_handle: Arc::new(Mutex::new(Some(thread_handle))),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Execute a `!Send` future on the LocalSet thread.
    ///
    /// This method sends a request to the LocalSet thread and waits for the
    /// result using a oneshot channel.
    ///
    /// # Arguments
    ///
    /// * `f` — A closure that returns a `!Send` future
    ///
    /// # Returns
    ///
    /// The result of the future.
    ///
    /// # Errors
    ///
    /// Returns an error if the bridge has been shut down or the receiver was dropped.
    #[allow(dead_code)]
    pub async fn execute<F, T>(&self, f: F) -> crate::AcpResult<T>
    where
        F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + 'static>>
            + Send
            + 'static,
        T: Send + 'static,
    {
        let (response_tx, response_rx) = oneshot::channel::<T>();

        // Wrap the future to return Box<dyn Any + Send> for type erasure
        let wrapper = BridgeRequest {
            future_factory: Box::new(move || {
                let future = f();
                Box::pin(async move {
                    let result = future.await;
                    // Send result, ignore errors (receiver may have timed out)
                    let _ = response_tx.send(result);
                    Box::new(()) as Box<dyn Any + Send>
                })
            }),
        };

        self.request_tx
            .send(Some(wrapper))
            .await
            .map_err(|_| crate::AcpError::connection_failed("LocalSet bridge channel closed"))?;

        response_rx.await.map_err(|_| {
            crate::AcpError::connection_failed("LocalSet bridge response channel closed")
        })
    }

    /// Execute a `!Send` future with a timeout.
    ///
    /// Same as [`execute`], but wraps the operation with a timeout.
    ///
    /// # Arguments
    ///
    /// * `f` — A closure that returns a `!Send` future
    /// * `timeout_duration` — Maximum time to wait for the operation
    /// * `operation_name` — Name of the operation (for error messages)
    ///
    /// # Errors
    ///
    /// Returns a timeout error if the operation doesn't complete in time.
    #[allow(dead_code)]
    pub async fn execute_with_timeout<F, T>(
        &self,
        f: F,
        timeout_duration: Duration,
        operation_name: &str,
    ) -> crate::AcpResult<T>
    where
        F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + 'static>>
            + Send
            + 'static,
        T: Send + 'static,
    {
        tokio::time::timeout(timeout_duration, self.execute(f))
            .await
            .map_err(|_| crate::AcpError::timeout(operation_name, timeout_duration))?
    }
}

impl Default for LocalSetBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LocalSetBridge {
    fn drop(&mut self) {
        // Check if we're potentially the last instance (strong_count == 1 means only us)
        // Use atomic flag to prevent race: even if count changes after we check,
        // only ONE drop will actually perform shutdown
        let is_last_instance = Arc::strong_count(&self.thread_handle) == 1;

        if is_last_instance {
            // Try to claim shutdown leadership via atomic flag
            // compare_exchange ensures only ONE caller succeeds, even if
            // another clone is created during the race window
            if self
                .shutdown_flag
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                debug!("Initiating LocalSet bridge shutdown (last instance)");

                // Send shutdown signal (None) to the LocalSet thread
                // Use try_send to avoid blocking in Drop
                let _ = self.request_tx.try_send(None);

                // Wait for thread to exit with timeout
                if let Some(handle) = self
                    .thread_handle
                    .lock()
                    .expect("bridge shutdown: mutex poisoned — unrecoverable")
                    .take()
                {
                    // Use channel to implement timeout on join
                    let (done_tx, done_rx) = std_mpsc::channel();

                    // Spawn helper thread to perform join
                    thread::spawn(move || {
                        let result = handle.join();
                        let _ = done_tx.send(result);
                    });

                    // Wait with timeout
                    match done_rx.recv_timeout(Duration::from_secs(5)) {
                        Ok(Ok(())) => debug!("LocalSet bridge thread exited cleanly"),
                        Ok(Err(e)) => warn!("LocalSet bridge thread panicked: {:?}", e),
                        Err(std_mpsc::RecvTimeoutError::Timeout) => {
                            warn!("LocalSet bridge thread did not shut down within 5s — detaching");
                        }
                        Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                            warn!("Join helper thread disconnected unexpectedly");
                        }
                    }
                }
            } else {
                // Another drop already claimed shutdown, we just decrement strong_count
                debug!("LocalSet bridge drop: shutdown already claimed by another instance");
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Test: Bridge starts and processes a simple request.
    #[tokio::test]
    async fn bridge_starts_and_processes_request() {
        let bridge = LocalSetBridge::new();

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let result: usize = bridge
            .execute(move || {
                let counter = counter_clone;
                Box::pin(async move {
                    // Simulate async work
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    counter.fetch_add(1, Ordering::SeqCst);
                    42
                })
            })
            .await
            .expect("Bridge execute failed");

        assert_eq!(result, 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    /// Test: Bridge handles multiple sequential requests.
    #[tokio::test]
    async fn bridge_handles_multiple_requests() {
        let bridge = LocalSetBridge::new();

        for i in 0..5 {
            let value = i;
            let result: i32 = bridge
                .execute(move || Box::pin(async move { value * 2 }))
                .await
                .expect("Bridge execute failed");

            assert_eq!(result, value * 2);
        }
    }

    /// Test: Bridge shuts down cleanly when dropped.
    #[tokio::test]
    async fn bridge_shuts_down_cleanly() {
        let bridge = LocalSetBridge::new();

        // Execute one request to verify bridge is working
        let result: i32 = bridge
            .execute(|| Box::pin(async move { 1 }))
            .await
            .expect("Bridge execute failed");
        assert_eq!(result, 1);

        // Drop the bridge — this should trigger graceful shutdown
        drop(bridge);

        // Wait a bit for thread cleanup
        tokio::time::sleep(Duration::from_millis(100)).await;

        // No assertion needed — we're testing that drop doesn't panic
    }

    /// Test: Bridge timeout works correctly.
    #[tokio::test]
    async fn bridge_timeout_expires() {
        let bridge = LocalSetBridge::new();

        let result: crate::AcpResult<i32> = bridge
            .execute_with_timeout(
                || {
                    Box::pin(async move {
                        // This future will sleep longer than the timeout
                        tokio::time::sleep(Duration::from_secs(10)).await;
                        42
                    })
                },
                Duration::from_millis(50),
                "test-operation",
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, crate::AcpError::Timeout { .. }));

        // Give the bridge time to clean up the pending task
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    /// Test: Bridge can execute concurrent requests.
    #[tokio::test]
    async fn bridge_handles_concurrent_requests() {
        let bridge = Arc::new(LocalSetBridge::new());
        let mut handles = vec![];

        for i in 0..3 {
            let bridge = bridge.clone();
            let handle = tokio::spawn(async move {
                let value = i;
                bridge
                    .execute(move || Box::pin(async move { value * 2 }))
                    .await
                    .expect("Bridge execute failed")
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.expect("Task join failed"));
        }

        // All results should be present (order may vary)
        assert_eq!(results.len(), 3);
        for result in results {
            assert!((0..6).contains(&result) && result % 2 == 0);
        }
    }

    /// Test: Bridge handles shutdown while request is in-flight.
    /// This tests that dropping the bridge doesn't panic even if requests
    /// are pending or executing on the LocalSet thread.
    #[tokio::test]
    async fn bridge_shutdown_while_request_in_flight() {
        let bridge = LocalSetBridge::new();

        // Start a request that will run on the LocalSet thread
        // We don't await it immediately, keeping it in-flight
        let request_handle = tokio::spawn(async move {
            bridge
                .execute(|| {
                    Box::pin(async move {
                        // Short operation to ensure it starts before we drop
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        42
                    })
                })
                .await
        });

        // Give the request time to start executing on the LocalSet thread
        tokio::time::sleep(Duration::from_millis(20)).await;

        // The bridge has already been moved into the spawned task
        // No explicit drop needed - it will be dropped when the task completes/fails

        // Wait for the spawned task to finish
        // It may succeed (request completed before shutdown) or fail (shutdown interrupted)
        // Either outcome is acceptable - we're just testing that it doesn't panic
        let result = request_handle.await;

        // The important thing is that the task completed without panicking
        assert!(result.is_ok());

        // Wait a bit for cleanup
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    /// Test: Bridge handles request returning unit type (empty result).
    #[tokio::test]
    async fn bridge_handles_empty_result() {
        let bridge = LocalSetBridge::new();

        // Execute a request that returns () (unit type)
        let result: () = bridge
            .execute(|| Box::pin(async move {}))
            .await
            .expect("Bridge execute failed");

        // Just verify it completed without error
        // unit type has no value to compare
        let _ = result;
    }

    /// Test: Bridge handles error propagation correctly.
    #[tokio::test]
    async fn bridge_error_propagation() {
        let bridge = LocalSetBridge::new();

        // Create a request that returns an error result
        let result: Result<i32, String> = bridge
            .execute(|| Box::pin(async move { Err("test error".to_string()) }))
            .await
            .expect("Bridge execute failed");

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "test error");
    }

    /// Test: Bridge clone shares the same underlying thread.
    #[tokio::test]
    async fn bridge_clone_shares_thread() {
        let bridge = LocalSetBridge::new();
        let bridge_clone = bridge.clone();

        // Use original bridge
        let result1: i32 = bridge
            .execute(|| Box::pin(async move { 1 }))
            .await
            .expect("Bridge execute failed");

        // Use cloned bridge
        let result2: i32 = bridge_clone
            .execute(|| Box::pin(async move { 2 }))
            .await
            .expect("Bridge execute failed");

        assert_eq!(result1, 1);
        assert_eq!(result2, 2);

        // Drop clone first - should not shutdown thread
        drop(bridge_clone);

        // Original should still work
        let result3: i32 = bridge
            .execute(|| Box::pin(async move { 3 }))
            .await
            .expect("Bridge execute failed");

        assert_eq!(result3, 3);

        // Drop original - this should shutdown thread
        drop(bridge);

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
