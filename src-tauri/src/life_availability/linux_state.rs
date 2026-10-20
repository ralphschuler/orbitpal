//! Pure login1 policy. LockedHint is the desktop's reported hint, not a
//! compositor-independent proof that every Linux screen locker is visible.
use super::{Observation, Reason};

pub(super) struct SessionSample<'a> {
    pub locked: Option<bool>,
    pub active: Option<bool>,
    pub state: Option<&'a str>,
    pub preparing_sleep: Option<bool>,
}

pub(super) fn interpret(sample: SessionSample<'_>) -> Observation {
    let (Some(locked), Some(active), Some(state), Some(sleeping)) = (
        sample.locked,
        sample.active,
        sample.state,
        sample.preparing_sleep,
    ) else {
        return Observation::unknown(Reason::ObservationUnavailable);
    };
    if !matches!(state, "active" | "online" | "closing") {
        return Observation::unknown(Reason::ObservationUnavailable);
    }
    if sleeping {
        Observation::blocked(Reason::Sleeping)
    } else if locked {
        Observation::blocked(Reason::Locked)
    } else if !active || state != "active" {
        Observation::blocked(Reason::InactiveSession)
    } else {
        Observation::ready()
    }
}

pub(super) fn concrete_session_path(path: &str) -> bool {
    let Some(id) = path.strip_prefix("/org/freedesktop/login1/session/") else {
        return false;
    };
    !id.is_empty()
        && id.len() <= 128
        && !matches!(id, "self" | "auto")
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[derive(Clone, Copy)]
pub(super) enum Event<'a> {
    Locked(Option<bool>),
    Active(Option<bool>),
    State(Option<&'a str>),
    Sleep(Option<bool>),
    Invalidated,
}
pub(super) fn event_reason(event: Event<'_>) -> Reason {
    match event {
        Event::Locked(Some(true)) => Reason::Locked,
        Event::Active(Some(false)) => Reason::InactiveSession,
        Event::State(Some("online" | "closing")) => Reason::InactiveSession,
        Event::Sleep(Some(true)) => Reason::Sleeping,
        Event::Locked(Some(false))
        | Event::Active(Some(true))
        | Event::State(Some("active"))
        | Event::Sleep(Some(false)) => Reason::Initializing,
        _ => Reason::ObservationUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life_availability::Availability;
    fn sample() -> SessionSample<'static> {
        SessionSample {
            locked: Some(false),
            active: Some(true),
            state: Some("active"),
            preparing_sleep: Some(false),
        }
    }
    #[test]
    fn current_unlocked_session_is_ready_without_a_future_unlock_event() {
        assert_eq!(interpret(sample()), Observation::ready());
    }
    #[test]
    fn missing_or_unknown_properties_never_mean_unlocked() {
        for field in 0..4 {
            let mut value = sample();
            match field {
                0 => value.locked = None,
                1 => value.active = None,
                2 => value.state = None,
                _ => value.preparing_sleep = None,
            }
            assert_eq!(interpret(value).state, Availability::Unknown);
        }
        for state in ["", "opening", "ACTIVE", "unknown", "active\0"] {
            let mut value = sample();
            value.state = Some(state);
            assert_eq!(interpret(value).state, Availability::Unknown);
        }
    }
    #[test]
    fn locked_sleeping_inactive_and_inconsistent_status_all_deny() {
        let mut value = sample();
        value.locked = Some(true);
        assert_eq!(interpret(value).reason, Reason::Locked);
        let mut value = sample();
        value.preparing_sleep = Some(true);
        assert_eq!(interpret(value).reason, Reason::Sleeping);
        let mut value = sample();
        value.active = Some(false);
        assert_eq!(interpret(value).reason, Reason::InactiveSession);
        for state in ["online", "closing"] {
            let mut value = sample();
            value.state = Some(state);
            assert_eq!(interpret(value).reason, Reason::InactiveSession);
        }
    }
    #[test]
    fn observer_clears_are_only_resample_requests() {
        for event in [
            Event::Locked(Some(false)),
            Event::Active(Some(true)),
            Event::State(Some("active")),
            Event::Sleep(Some(false)),
        ] {
            assert_eq!(event_reason(event), Reason::Initializing);
        }
        for event in [
            Event::Locked(None),
            Event::Active(None),
            Event::State(Some("unknown")),
            Event::Sleep(None),
            Event::Invalidated,
        ] {
            assert_eq!(event_reason(event), Reason::ObservationUnavailable);
        }
    }
    #[test]
    fn convenience_aliases_and_unbounded_or_other_object_paths_are_rejected() {
        for path in [
            "/org/freedesktop/login1/session/1",
            "/org/freedesktop/login1/session/c2",
            "/org/freedesktop/login1/session/_32",
        ] {
            assert!(concrete_session_path(path));
        }
        for path in [
            "/org/freedesktop/login1/session/self",
            "/org/freedesktop/login1/session/auto",
            "/org/freedesktop/login1/session/",
            "/org/freedesktop/login1/session/1/other",
            "/org/freedesktop/login1/seat/seat0",
        ] {
            assert!(!concrete_session_path(path));
        }
        assert!(!concrete_session_path(&format!(
            "/org/freedesktop/login1/session/{}",
            "a".repeat(129)
        )));
    }
}
