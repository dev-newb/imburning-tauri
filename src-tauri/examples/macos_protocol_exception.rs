//! Run with `cargo run --release --example macos_protocol_exception` on macOS.
//! Cargo's test harness always enables unwinding, even for `cargo test --release`,
//! so this must be an ordinary executable built with the application's profile.
//!
//! Wry catches Objective-C exceptions around WKURLSchemeTask's didReceive*/
//! didFinish calls when WebKit cancels a request. Exercise that same catch/raise
//! boundary on a Tokio worker, without opening a webview or accessing user data.

#[cfg(target_os = "macos")]
fn main() {
    use objc2::{exception, msg_send, rc::autoreleasepool};
    use objc2_foundation::{ns_string, NSException, NSInternalInconsistencyException};
    use std::panic::AssertUnwindSafe;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("test runtime");
    runtime.block_on(async {
        tokio::spawn(async {
            for _ in 0..100 {
                autoreleasepool(|_| {
                    let result = exception::catch(AssertUnwindSafe(|| unsafe {
                        let error = NSException::exceptionWithName_reason_userInfo(
                            NSInternalInconsistencyException,
                            Some(ns_string!("This task has already been stopped")),
                            None,
                        );
                        // Raise in Objective-C, as WKURLSchemeTask does. A Rust
                        // panic would not exercise the foreign-exception path.
                        let _: () = msg_send![&*error, raise];
                    }));
                    let error = result
                        .expect_err("cancelled request must throw")
                        .expect("Objective-C exception object");
                    assert!(format!("{error}").contains("This task has already been stopped"));
                    assert_eq!(exception::catch(|| 42).expect("next request succeeds"), 42);
                });
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("worker survives cancelled requests");
        assert_eq!(
            tokio::spawn(async { 42 })
                .await
                .expect("runtime remains usable"),
            42
        );
    });
    println!(
        "PASS: 100 Objective-C cancellation exceptions caught; worker and runtime remain usable"
    );
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("SKIP: this release-mode regression check requires macOS");
}
