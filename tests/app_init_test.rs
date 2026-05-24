use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::OnceCell;

/// Regression test: `build_app` must use `OnceCell::get_or_try_init` so concurrent
/// serverless invocations do not race through initialization (e.g. double crypto install).
#[tokio::test]
async fn once_cell_get_or_try_init_runs_initializer_once_under_concurrency() {
    static CELL: OnceCell<u32> = OnceCell::const_new();
    static INIT_CALLS: AtomicUsize = AtomicUsize::new(0);

    let init = || async {
        INIT_CALLS.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        Ok::<_, anyhow::Error>(42)
    };

    let (a, b, c) = tokio::join!(
        CELL.get_or_try_init(init),
        CELL.get_or_try_init(init),
        CELL.get_or_try_init(init),
    );

    assert_eq!(INIT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(*a.unwrap(), 42);
    assert_eq!(*b.unwrap(), 42);
    assert_eq!(*c.unwrap(), 42);
}
