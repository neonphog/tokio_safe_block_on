#![deny(warnings)]
#![deny(missing_docs)]
#![allow(clippy::needless_doctest_main)]
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
/// This version will never time out - you may end up binding a
/// tokio background thread forever.
pub fn tokio_safe_block_forever_on<F: std::future::Future>(f: F) -> F::Output {
    // work around pin requirements with a Box
    let f = Box::pin(f);

    let handle = tokio::runtime::Handle::current();
    // first, we need to make sure to move this thread to the background
    tokio::task::block_in_place(move || {
        // poll until we get a result
        // futures::executor::block_on(async move { f.await })
        handle.block_on(async move { f.await })
    })
}

/// Provides the ability to execute async code from a sync context,
/// without blocking a tokio core thread or busy looping the cpu.
/// You must ensure you are within the context of a tokio::task,
/// This allows `tokio::task::block_in_place` to move to a blocking thread.
pub fn tokio_safe_block_on<F: std::future::Future>(
    f: F,
    timeout: std::time::Duration,
) -> Result<F::Output, BlockOnError> {
    // work around pin requirements with a Box
    let f = Box::pin(f);

    let handle = tokio::runtime::Handle::current();
    // first, we need to make sure to move this thread to the background
    tokio::task::block_in_place(move || {
        // poll until we get a result or a timeout
        // futures::executor::block_on(async move {
        handle.block_on(async move {
            match futures::future::select(f, tokio::time::delay_for(timeout)).await {
                futures::future::Either::Left((res, _)) => Ok(res),
                futures::future::Either::Right(_) => Err(BlockOnError::Timeout),
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(threaded_scheduler)]
    async fn it_should_execute_async_from_sync_context_forever() {
        tokio::task::spawn(async move {
            (|| {
                let result = tokio_safe_block_forever_on(async move { "test0" });
                assert_eq!("test0", result);
            })()
        })
        .await
        .unwrap();
    }

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
