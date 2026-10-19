//! Keep an OS permission dialog single-flight even when its caller disappears
//! or times out. The queued native operation/completion owns the guard; dropping
//! the waiting future must not allow another dialog while macOS is still busy.
use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};

#[derive(Default)]
pub(crate) struct RequestGate(Arc<AtomicBool>);

pub(crate) struct RequestGuard(Arc<AtomicBool>);

#[cfg(target_os = "macos")]
static REQUESTS: std::sync::LazyLock<RequestGate> = std::sync::LazyLock::new(Default::default);

#[cfg(target_os = "macos")]
pub(crate) fn begin() -> Result<RequestGuard, String> {
    REQUESTS.begin("Eine Systemfreigabe wird bereits angefragt. Bitte zuerst den macOS-Dialog beantworten oder die Systemeinstellungen prüfen.")
}

impl RequestGate {
    pub(crate) fn begin(&self, message: &str) -> Result<RequestGuard, String> {
        self.0.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| message.to_owned())?;
        Ok(RequestGuard(self.0.clone()))
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); }
}

pub(crate) async fn receive<T>(
    pending: tokio::sync::oneshot::Receiver<Result<T, String>>,
    timeout: Duration,
    timeout_message: &str,
    closed_message: &str,
) -> Result<T, String> {
    tokio::time::timeout(timeout, pending).await
        .map_err(|_| timeout_message.to_owned())?
        .map_err(|_| closed_message.to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_holds_its_gate_until_native_work_completes() {
        let gate = RequestGate::default();
        let guard = gate.begin("already pending").unwrap();
        assert_eq!(gate.begin("already pending").err().unwrap(), "already pending");
        drop(guard);
        assert!(gate.begin("already pending").is_ok());
    }

    #[tokio::test]
    async fn timeout_cannot_release_a_still_running_native_request() {
        let gate = RequestGate::default();
        let native_guard = gate.begin("pending").unwrap();
        let (native_send, pending) = tokio::sync::oneshot::channel::<Result<(), String>>();
        assert_eq!(receive(pending, Duration::ZERO, "timeout", "closed").await.unwrap_err(), "timeout");
        assert!(native_send.is_closed());
        assert!(gate.begin("pending").is_err());
        drop(native_guard);
        assert!(gate.begin("pending").is_ok());
    }

    #[tokio::test]
    async fn native_errors_completion_and_cancellation_propagate_without_os_access() {
        let (send, pending) = tokio::sync::oneshot::channel();
        send.send(Ok(7)).unwrap();
        assert_eq!(receive(pending, Duration::from_secs(1), "timeout", "closed").await.unwrap(), 7);
        let (send, pending) = tokio::sync::oneshot::channel::<Result<(), String>>();
        send.send(Err("native failure".into())).unwrap();
        assert_eq!(receive(pending, Duration::from_secs(1), "timeout", "closed").await.unwrap_err(), "native failure");
        let (send, pending) = tokio::sync::oneshot::channel::<Result<(), String>>();
        drop(send);
        assert_eq!(receive(pending, Duration::from_secs(1), "timeout", "closed").await.unwrap_err(), "closed");
    }
}
