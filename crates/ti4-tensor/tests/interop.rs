//! The one determinism setting that needs a process to itself.
//!
//! libtorch permits `set_num_interop_threads` once, before any parallel work has started. An
//! integration test binary is its own process, and this is the only test in it, so the call here
//! is genuinely the first — which is what makes the assertion meaningful. Adding a second test to
//! this file would silently invalidate it.

#[test]
fn the_inter_op_pool_is_pinned_when_configured_first() {
    let reported = ti4_tensor::configure_deterministic(20_260_821).expect("configured");
    assert_eq!(
        reported.inter_op_threads, 1,
        "the inter-op pool was not pinned even though this call was the process's first"
    );
    assert_eq!(reported.intra_op_threads, 1);
    assert!(!reported.cuda, "CUDA is available to a CPU-only branch");
}
