//! Typed projection only. Native CF extraction must supply None for any absent
//! or wrong-typed key; these fields never cross the public event boundary.
use super::{Observation, Reason};
pub(super) struct SessionSample {
    pub uid: Option<u32>,
    pub process_uid: u32,
    pub on_console: Option<bool>,
    pub login_done: Option<bool>,
    pub locked: Option<bool>,
    pub protected_data: Option<bool>,
    pub sleeping: bool,
    pub inactive: bool,
    pub protected_blocked: bool,
}
pub(super) fn parse(sample: SessionSample) -> Observation {
    if sample.sleeping {
        return Observation::blocked(Reason::Sleeping);
    }
    if sample.inactive {
        return Observation::blocked(Reason::InactiveSession);
    }
    if sample.protected_blocked || sample.protected_data == Some(false) {
        return Observation::blocked(Reason::ProtectedDataUnavailable);
    }
    if sample.uid != Some(sample.process_uid)
        || sample.on_console.is_none()
        || sample.login_done.is_none()
        || sample.locked.is_none()
        || sample.protected_data.is_none()
    {
        return Observation::unknown(Reason::ObservationUnavailable);
    }
    if sample.on_console != Some(true) || sample.login_done != Some(true) {
        return Observation::blocked(Reason::InactiveSession);
    }
    if sample.locked == Some(true) {
        Observation::blocked(Reason::Locked)
    } else {
        Observation::ready()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::life_availability::Availability;
    fn valid() -> SessionSample {
        SessionSample {
            uid: Some(501),
            process_uid: 501,
            on_console: Some(true),
            login_done: Some(true),
            locked: Some(false),
            protected_data: Some(true),
            sleeping: false,
            inactive: false,
            protected_blocked: false,
        }
    }
    #[test]
    fn initial_fresh_false_works_without_a_future_unlock_notification() {
        assert_eq!(parse(valid()).state, Availability::AwakeUnlocked);
        let mut locked = valid();
        locked.locked = Some(true);
        assert_eq!(parse(locked).reason, Reason::Locked);
    }
    #[test]
    fn missing_wrong_typed_or_foreign_fields_never_default_to_unlocked() {
        for field in 0..6 {
            let mut value = valid();
            match field {
                0 => value.uid = None,
                1 => value.uid = Some(502),
                2 => value.on_console = None,
                3 => value.login_done = None,
                4 => value.locked = None,
                _ => value.protected_data = None,
            }
            assert_eq!(parse(value).state, Availability::Unknown);
        }
    }
    #[test]
    fn public_sleep_session_and_protected_data_are_additional_blockers() {
        let mut value = valid();
        value.sleeping = true;
        assert_eq!(parse(value).reason, Reason::Sleeping);
        let mut value = valid();
        value.inactive = true;
        assert_eq!(parse(value).reason, Reason::InactiveSession);
        let mut value = valid();
        value.protected_data = Some(false);
        assert_eq!(parse(value).reason, Reason::ProtectedDataUnavailable);
    }
}
