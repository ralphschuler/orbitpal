//! Pure interpretation of the current process's Windows session observation.
//! Native buffers/names never leave the adapter. Windows 7's reversed flags
//! are deliberately unsupported; OrbitPal/Tauri targets Windows 10 and later.
use super::{Availability, Observation, Reason};

pub(super) fn interpret(
    expected_session: u32,
    returned_session: u32,
    level: u32,
    connection: i32,
    flags: i32,
) -> Observation {
    if level != 1 || expected_session == u32::MAX || returned_session != expected_session {
        return Observation {
            state: Availability::Unknown,
            reason: Reason::ObservationUnavailable,
        };
    }
    // WTSActive = 0. Physical console and active RDP sessions are both valid;
    // never select another user's active console session as a substitute.
    if connection != 0 {
        return Observation {
            state: Availability::SuspendedLocked,
            reason: Reason::InactiveSession,
        };
    }
    match flags {
        0 => Observation {
            state: Availability::SuspendedLocked,
            reason: Reason::Locked,
        },
        1 => Observation {
            state: Availability::AwakeUnlocked,
            reason: Reason::Ready,
        },
        _ => Observation {
            state: Availability::Unknown,
            reason: Reason::ObservationUnavailable,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlocked_initial_sample_needs_no_prior_unlock_notification() {
        let sample = interpret(42, 42, 1, 0, 1);
        assert_eq!(sample.state, Availability::AwakeUnlocked);
        assert_eq!(sample.reason, Reason::Ready);
    }

    #[test]
    fn initial_lock_and_disconnected_sessions_block() {
        assert_eq!(interpret(42, 42, 1, 0, 0).reason, Reason::Locked);
        for state in [1, 2, 3, 4, 5, 6, 7, 8, 9, -1] {
            assert_eq!(
                interpret(42, 42, 1, state, 1).reason,
                Reason::InactiveSession
            );
        }
    }

    #[test]
    fn unknown_flags_wrong_level_or_other_session_never_prove_unlocked() {
        for sample in [
            interpret(42, 42, 1, 0, -1),
            interpret(42, 42, 1, 0, 2),
            interpret(42, 43, 1, 0, 1),
            interpret(42, 42, 2, 0, 1),
            interpret(u32::MAX, u32::MAX, 1, 0, 1),
        ] {
            assert_eq!(sample.state, Availability::Unknown);
        }
    }
}
