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
#[derive(Debug)]
pub enum BlockOnError {
    /// The future did not complete within the time alloted.
    Timeout,

    /// The spawned tokio task returned a JoinError
    TaskJoinError(tokio::task::JoinError),
}

impl std::fmt::Display for BlockOnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<tokio::task::JoinError> for BlockOnError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self::TaskJoinError(e)
    }
}

impl std::error::Error for BlockOnError {}

/// Provides the ability to execute async code from a sync context,
/// without blocking a tokio core thread or busy looping the cpu.
/// You must ensure you are within the context of a tokio::task,
/// This allows `tokio::task::block_in_place` to move to a blocking thread.
/// This version will never time out - you may end up binding a
/// tokio background thread forever.
pub fn tokio_safe_block_forever_on<F>(f: F) -> Result<F::Output, BlockOnError>
where
    F: 'static + std::future::Future + Send,
    <F as std::future::Future>::Output: Send,
{
    // first, we need to make sure to move this thread to the background
    tokio::task::block_in_place(move || {
        // poll until we get a result
        futures::executor::block_on(async move {
            // if we don't spawn, any recursive calls to block_in_place
            // will fail
            Ok(tokio::task::spawn(f).await?)
        })
    })
}

/// Provides the ability to execute async code from a sync context,
/// without blocking a tokio core thread or busy looping the cpu.
/// You must ensure you are within the context of a tokio::task,
/// This allows `tokio::task::block_in_place` to move to a blocking thread.
pub fn tokio_safe_block_on<F>(
    f: F,
    timeout: std::time::Duration,
) -> Result<F::Output, BlockOnError>
where
    F: 'static + std::future::Future + Send,
    <F as std::future::Future>::Output: Send,
{
    // work around pin requirements with a Box
    let f = Box::pin(f);

    // apply the timeout
    let f = async move {
        match futures::future::select(f, tokio::time::delay_for(timeout)).await
        {
            futures::future::Either::Left((res, _)) => Ok(res),
            futures::future::Either::Right(_) => Err(BlockOnError::Timeout),
        }
    };

    // execute the future
    tokio_safe_block_forever_on(f)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(threaded_scheduler)]
    async fn it_should_execute_async_from_sync_context_forever() {
        tokio::task::spawn(async move {
            (|| {
                let result =
                    tokio_safe_block_forever_on(async move { "test0" });
                assert_eq!("test0", result.unwrap());
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
                        tokio::time::delay_for(
                            std::time::Duration::from_millis(2),
                        )
                        .await;
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
                        tokio::time::delay_for(
                            std::time::Duration::from_millis(10),
                        )
                        .await;
                        "test3"
                    },
                    std::time::Duration::from_millis(2),
                );
                assert_matches::assert_matches!(
                    result,
                    Err(BlockOnError::Timeout)
                );
            })()
        })
        .await
        .unwrap();
    }

    #[tokio::test(threaded_scheduler)]
    async fn recursive_blocks_test() {
        async fn rec_async(depth: u8) -> u8 {
            if depth >= 10 {
                return depth;
            }
            rec_sync(depth + 1)
        }

        fn rec_sync(depth: u8) -> u8 {
            tokio_safe_block_forever_on(
                async move { rec_async(depth + 1).await },
            )
            .unwrap()
        }

        tokio::task::spawn(async move {
            assert_eq!(10, rec_async(0).await);
        })
        .await
        .unwrap();
    }
}
