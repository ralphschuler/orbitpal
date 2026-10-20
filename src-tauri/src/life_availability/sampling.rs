use super::{AvailabilityGate, Observation, Snapshot};
#[derive(Default)]
pub struct SerializedSampler {
    serial: tokio::sync::Mutex<()>,
}
impl SerializedSampler {
    pub async fn sample(
        &self,
        gate: &AvailabilityGate,
        operation: impl std::future::Future<Output = Observation>,
    ) -> Snapshot {
        let _serial = self.serial.lock().await;
        let fence = gate.sample_fence();
        gate.apply_sample(fence, operation.await)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::life_availability::{Availability, Reason, Source};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    #[tokio::test]
    async fn overlapping_probe_is_not_polled_until_first_sample_is_committed() {
        let gate = Arc::new(AvailabilityGate::new(Source::WindowsWts));
        let sampler = Arc::new(SerializedSampler::default());
        let (start_tx, start_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let first = {
            let gate = gate.clone();
            let sampler = sampler.clone();
            tokio::spawn(async move {
                sampler
                    .sample(&gate, async {
                        let _ = start_tx.send(());
                        release_rx.await.unwrap();
                        Observation::ready()
                    })
                    .await
            })
        };
        start_rx.await.unwrap();
        let polled = Arc::new(AtomicBool::new(false));
        let second = {
            let gate = gate.clone();
            let sampler = sampler.clone();
            let polled = polled.clone();
            tokio::spawn(async move {
                sampler
                    .sample(&gate, async {
                        polled.store(true, Ordering::SeqCst);
                        Observation::blocked(Reason::Locked)
                    })
                    .await
            })
        };
        tokio::task::yield_now().await;
        assert!(!polled.load(Ordering::SeqCst));
        release_tx.send(()).unwrap();
        first.await.unwrap();
        second.await.unwrap();
        assert_eq!(gate.snapshot().state, Availability::SuspendedLocked);
    }
}
