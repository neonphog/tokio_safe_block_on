![Crates.io](https://img.shields.io/crates/l/tokio_safe_block_on)
![Crates.io](https://img.shields.io/crates/v/tokio_safe_block_on)

# tokio_safe_block_on

Provides the ability to execute async code from a sync context,
without blocking a tokio core thread or busy looping the cpu.

## Example

```rust
#[tokio::main(flavor = "multi_thread")]
async fn main() {
    // we need to ensure we are in the context of a tokio task
    tokio::task::spawn(async move {
        // some library api may take a sync callback
        // but we want to be able to execute async code
        (|| {
            let r = tokio_safe_block_on::tokio_safe_block_on(
                // async code to poll synchronously
                async move {
                    // simulate some async work
                    tokio::time::sleep(
                        std::time::Duration::from_millis(2)
                    ).await;

                    // return our result
                    "test"
                },

                // timeout to allow async execution
                std::time::Duration::from_millis(10),
            ).unwrap();

            // note we get the result inline with no `await`
            assert_eq!("test", r);
        })()
    })
    .await
    .unwrap();
}
```
