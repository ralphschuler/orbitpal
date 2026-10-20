//! Bounded, generation-fenced native availability. Clear notifications are not
//! observations; adapters must sample actual current-session state.
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, Weak,
};

const MAX_GENERATION: u64 = 9_007_199_254_740_991;
const MAX_WATCHERS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    AwakeUnlocked,
    SuspendedLocked,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Ready,
    Initializing,
    Locked,
    Sleeping,
    InactiveSession,
    ProtectedDataUnavailable,
    ObservationUnavailable,
    MonitorDisconnected,
    Unsupported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    MacosCgSessionCompatibility,
    WindowsWts,
    LinuxLogind,
    Unsupported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Observation {
    pub state: Availability,
    pub reason: Reason,
}
impl Observation {
    pub fn ready() -> Self {
        Self {
            state: Availability::AwakeUnlocked,
            reason: Reason::Ready,
        }
    }
    pub fn unknown(reason: Reason) -> Self {
        Self {
            state: Availability::Unknown,
            reason,
        }
    }
    pub fn blocked(reason: Reason) -> Self {
        Self {
            state: Availability::SuspendedLocked,
            reason,
        }
    }
    fn checked(self) -> Self {
        match (self.state, self.reason) {
            (Availability::AwakeUnlocked, Reason::Ready) => self,
            (
                Availability::SuspendedLocked,
                Reason::Locked
                | Reason::Sleeping
                | Reason::InactiveSession
                | Reason::ProtectedDataUnavailable,
            ) => self,
            (Availability::Unknown, reason) if reason != Reason::Ready => self,
            _ => Self::unknown(Reason::ObservationUnavailable),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub state: Availability,
    pub reason: Reason,
    pub generation: u64,
    pub source: Source,
}
#[derive(Clone, Copy, Debug)]
pub struct Lease {
    generation: u64,
}
struct State {
    snapshot: Snapshot,
    watchers: Vec<Weak<AtomicBool>>,
    stopped: bool,
}
pub struct AvailabilityGate {
    state: Mutex<State>,
    source: Source,
}
impl AvailabilityGate {
    pub fn new(source: Source) -> Self {
        Self {
            source,
            state: Mutex::new(State {
                snapshot: Snapshot {
                    state: Availability::Unknown,
                    reason: Reason::Initializing,
                    generation: 1,
                    source,
                },
                watchers: Vec::new(),
                stopped: false,
            }),
        }
    }
    fn unavailable(&self) -> Snapshot {
        Snapshot {
            state: Availability::Unknown,
            reason: Reason::MonitorDisconnected,
            generation: MAX_GENERATION,
            source: self.source,
        }
    }
    fn invalidate(state: &mut State) {
        state.snapshot.generation = state
            .snapshot
            .generation
            .saturating_add(1)
            .min(MAX_GENERATION);
        for watcher in state.watchers.drain(..) {
            if let Some(cancel) = watcher.upgrade() {
                cancel.store(true, Ordering::SeqCst);
            }
        }
        if state.snapshot.generation == MAX_GENERATION {
            state.stopped = true;
            state.snapshot.state = Availability::Unknown;
            state.snapshot.reason = Reason::MonitorDisconnected;
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state
            .lock()
            .map(|state| state.snapshot)
            .unwrap_or_else(|_| self.unavailable())
    }
    pub fn sample_fence(&self) -> u64 {
        self.snapshot().generation
    }
    pub fn apply_sample(&self, fence: u64, observation: Observation) -> Snapshot {
        let Ok(mut state) = self.state.lock() else {
            return self.unavailable();
        };
        let observation = observation.checked();
        if state.stopped {
            return state.snapshot;
        }
        // Only a clear observation can grant authority. A concurrent or delayed
        // blocking observation is conservatively retained, never discarded just
        // because a clear sample won a race. A later fresh probe may clear it.
        if state.snapshot.generation != fence {
            if observation.state != Availability::AwakeUnlocked {
                Self::invalidate(&mut state);
                if !state.stopped {
                    state.snapshot.state = observation.state;
                    state.snapshot.reason = observation.reason;
                }
            }
            return state.snapshot;
        }
        if state.snapshot.state != observation.state || state.snapshot.reason != observation.reason
        {
            Self::invalidate(&mut state);
            if !state.stopped {
                state.snapshot.state = observation.state;
                state.snapshot.reason = observation.reason;
            }
        }
        state.snapshot
    }
    /// Blocking events advance the generation even if the enum did not change.
    /// A delayed unlocked sample cannot supersede any such event.
    pub fn block(&self, reason: Reason) -> Snapshot {
        let Ok(mut state) = self.state.lock() else {
            return self.unavailable();
        };
        Self::invalidate(&mut state);
        if !state.stopped {
            let observation = match reason {
                Reason::Locked
                | Reason::Sleeping
                | Reason::InactiveSession
                | Reason::ProtectedDataUnavailable => Observation::blocked(reason),
                _ => Observation::unknown(reason),
            }
            .checked();
            state.snapshot.state = observation.state;
            state.snapshot.reason = observation.reason;
        }
        state.snapshot
    }
    pub fn watch(&self, cancel: &Arc<AtomicBool>) -> Option<Lease> {
        let Ok(mut state) = self.state.lock() else {
            cancel.store(true, Ordering::SeqCst);
            return None;
        };
        state.watchers.retain(|watcher| {
            watcher
                .upgrade()
                .is_some_and(|cancel| !cancel.load(Ordering::SeqCst))
        });
        if state.stopped
            || state.snapshot.state != Availability::AwakeUnlocked
            || state.watchers.len() >= MAX_WATCHERS
            || cancel.load(Ordering::SeqCst)
        {
            cancel.store(true, Ordering::SeqCst);
            return None;
        }
        state.watchers.push(Arc::downgrade(cancel));
        Some(Lease {
            generation: state.snapshot.generation,
        })
    }
    pub fn current(&self, lease: Lease) -> bool {
        self.state
            .lock()
            .map(|state| {
                !state.stopped
                    && state.snapshot.state == Availability::AwakeUnlocked
                    && state.snapshot.generation == lease.generation
            })
            .unwrap_or(false)
    }
    pub fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            Self::invalidate(&mut state);
            state.stopped = true;
            state.snapshot.state = Availability::Unknown;
            state.snapshot.reason = Reason::MonitorDisconnected;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_initial_false_requires_no_unlock_event_and_missing_data_never_grants() {
        let gate = AvailabilityGate::new(Source::MacosCgSessionCompatibility);
        assert_eq!(gate.snapshot().state, Availability::Unknown);
        gate.apply_sample(gate.sample_fence(), Observation::ready());
        let cancel = Arc::new(AtomicBool::new(false));
        let lease = gate.watch(&cancel).unwrap();
        assert!(gate.current(lease));
        gate.apply_sample(
            gate.sample_fence(),
            Observation::unknown(Reason::ObservationUnavailable),
        );
        assert!(cancel.load(Ordering::SeqCst));
        assert!(!gate.current(lease));
    }
    #[test]
    fn blocked_event_beats_a_late_clear_and_stop_rejects_all_late_samples() {
        let gate = AvailabilityGate::new(Source::WindowsWts);
        let initial = gate.sample_fence();
        gate.block(Reason::Locked);
        assert_eq!(
            gate.apply_sample(initial, Observation::ready()).reason,
            Reason::Locked
        );
        let current = gate.sample_fence();
        gate.apply_sample(current, Observation::ready());
        let cancel = Arc::new(AtomicBool::new(false));
        let lease = gate.watch(&cancel).unwrap();
        gate.block(Reason::Sleeping);
        assert!(cancel.load(Ordering::SeqCst));
        assert!(!gate.current(lease));
        let wake = gate.block(Reason::Initializing).generation;
        gate.stop();
        assert_eq!(
            gate.apply_sample(wake, Observation::ready()).state,
            Availability::Unknown
        );
    }
    #[test]
    fn watcher_memory_is_bounded_and_invalid_observations_cannot_clear() {
        let gate = AvailabilityGate::new(Source::LinuxLogind);
        gate.apply_sample(gate.sample_fence(), Observation::ready());
        let cancels = (0..MAX_WATCHERS)
            .map(|_| Arc::new(AtomicBool::new(false)))
            .collect::<Vec<_>>();
        for cancel in &cancels {
            assert!(gate.watch(cancel).is_some());
        }
        assert!(gate.watch(&Arc::new(AtomicBool::new(false))).is_none());
        gate.apply_sample(
            gate.sample_fence(),
            Observation {
                state: Availability::AwakeUnlocked,
                reason: Reason::Locked,
            },
        );
        assert_eq!(gate.snapshot().state, Availability::Unknown);
        assert!(cancels.iter().all(|cancel| cancel.load(Ordering::SeqCst)));
    }
}
