//! The synchronous native setup hook is not necessarily inside Tokio after a
//! previous block_on returns. Use Tauri's owned runtime for the Linux monitor;
//! its task retains the same explicit stop/await lifecycle.
use std::future::Future;

pub(super) type TaskJoinHandle<T> = tauri::async_runtime::JoinHandle<T>;

pub(super) fn spawn<F>(future: F) -> TaskJoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tauri::async_runtime::spawn(future)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn owned_monitor_task_starts_from_plain_thread_without_entered_tokio() {
        let (sent, received) = mpsc::sync_channel(1);
        let task = std::thread::spawn(move || {
            assert!(tokio::runtime::Handle::try_current().is_err());
            // Mirror setup's completed overlay::start block_on: owning an
            // initialized Tauri runtime does not keep this thread entered.
            tauri::async_runtime::block_on(tokio::task::yield_now());
            assert!(tokio::runtime::Handle::try_current().is_err());
            // Exercise the same boundary as Monitor::start, with no AppHandle,
            // DBus connection, window, secret or platform session observation.
            spawn(async move {
                assert!(tokio::runtime::Handle::try_current().is_ok());
                tokio::time::timeout(Duration::from_secs(1), async {
                    tokio::task::yield_now().await;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                })
                .await
                .expect("synthetic monitor work must finish on its runtime");
                sent.send(41u8).unwrap();
                41u8
            })
        })
        .join()
        .expect("plain-thread monitor launch must not panic");
        assert_eq!(received.recv_timeout(Duration::from_secs(2)).unwrap(), 41);
        let result = tauri::async_runtime::block_on(async {
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .expect("owned monitor task must be joinable")
                .expect("synthetic monitor task must not panic")
        });
        assert_eq!(result, 41);
    }
}
