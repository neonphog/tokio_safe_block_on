#![deny(missing_docs)]
//! Provides the ability to execute async code from a sync context,
//! without blocking a tokio core thread or busy looping the cpu.
//!
//! # Example
//!
//! ```
//! #[tokio::main(threaded_scheduler)]
//! async fn main() {
//!     // we need to ensure we are in the context of a tokio task
//!     tokio::task::spawn(async move {
//!         // some library api may take a sync callback
//!         // but we want to be able to execute async code
//!         (|| {
//!             let r = tokio_safe_block_on::tokio_safe_block_on(
//!                 // async code to poll synchronously
//!                 async move {
//!                     // simulate some async work
//!                     tokio::time::delay_for(
//!                         std::time::Duration::from_millis(2)
//!                     ).await;
//!
//!                     // return our result
//!                     "test"
//!                 },
//!
//!                 // timeout to allow async execution
//!                 std::time::Duration::from_millis(10),
//!             ).unwrap();
//!
//!             // note we get the result inline with no `await`
//!             assert_eq!("test", r);
//!         })()
//!     })
//!     .await
//!     .unwrap();
//! }
//! ```

use std::sync::Arc;

/// internal thread park/unpark based waker
struct ThreadWaker(std::thread::Thread);

impl futures::task::ArcWake for ThreadWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.unpark();
    }
}

/// Error Type
#[derive(Debug, PartialEq)]
pub enum BlockOnError {
    /// The future did not complete within the time alloted.
    Timeout,
}

impl std::fmt::Display for BlockOnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for BlockOnError {}

/// Provides the ability to execute async code from a sync context,
/// without blocking a tokio core thread or busy looping the cpu.
/// You must ensure you are within the context of a tokio::task,
/// This allows `tokio::task::block_in_place` to move to a blocking thread.
pub fn tokio_safe_block_on<F: std::future::Future>(
    f: F,
    timeout: std::time::Duration,
) -> Result<F::Output, BlockOnError> {
    // work around pin requirements with a Box
    let mut f = Box::pin(f);

    // first, we need to make sure to move this thread to the background
    tokio::task::block_in_place(move || {
        // create a thread waker
        let waker = futures::task::waker(Arc::new(ThreadWaker(std::thread::current())));
        let mut context = std::task::Context::from_waker(&waker);

        // capture start time
        let start = std::time::Instant::now();

        // now, in the background poll the thread whenever we get a waker wake
        // (thread park occasionally spuriously wakes, but that's fine too)
        loop {
            let p = std::pin::Pin::new(&mut f);

            // poll our future
            match std::future::Future::poll(p, &mut context) {
                std::task::Poll::Pending => (),
                std::task::Poll::Ready(out) => return Ok(out),
            }

            // Pending... park the thread, or return timeout error
            match timeout.checked_sub(start.elapsed()) {
                Some(timeout) => std::thread::park_timeout(timeout),
                None => return Err(BlockOnError::Timeout),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(threaded_scheduler)]
    async fn it_should_execute_async_from_sync_context() {
        tokio::task::spawn(async move {
            (|| {
                let result = tokio_safe_block_on(
                    async move { "test1" },
                    std::time::Duration::from_millis(10),
                );
                assert_eq!("test1", result.unwrap());
            })()
        })
        .await
        .unwrap();
    }

    #[tokio::test(threaded_scheduler)]
    async fn it_should_execute_timed_async_from_sync_context() {
        tokio::task::spawn(async move {
            (|| {
                let result = tokio_safe_block_on(
                    async move {
                        tokio::time::delay_for(std::time::Duration::from_millis(2)).await;
                        "test2"
                    },
                    std::time::Duration::from_millis(10),
                );
                assert_eq!("test2", result.unwrap());
            })()
        })
        .await
        .unwrap();
    }

    #[tokio::test(threaded_scheduler)]
    async fn it_should_timeout_timed_async_from_sync_context() {
        tokio::task::spawn(async move {
            (|| {
                let result = tokio_safe_block_on(
                    async move {
                        tokio::time::delay_for(std::time::Duration::from_millis(10)).await;
                        "test3"
                    },
                    std::time::Duration::from_millis(2),
                );
                assert_eq!(BlockOnError::Timeout, result.unwrap_err());
            })()
        })
        .await
        .unwrap();
    }
}
