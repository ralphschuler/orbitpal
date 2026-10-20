//! Typed projection only. Required fields reject absence/wrong types. The
//! isolated SPI lock flag distinguishes complete-session absence from invalid
//! CF data; these fields never cross the public event boundary.
use super::{Observation, Reason};
#[derive(Clone, Copy)]
pub(super) enum LockFlag {
    Absent,
    Boolean(bool),
    Invalid,
}
pub(super) struct SessionSample {
    pub uid: Option<u32>,
    pub process_uid: u32,
    pub on_console: Option<bool>,
    pub login_done: Option<bool>,
    pub locked: LockFlag,
    pub lock_event: bool,
    pub protected_data: Option<bool>,
    pub sleeping: bool,
    pub inactive: bool,
    pub protected_blocked: bool,
}
pub(super) fn parse(sample: SessionSample) -> Observation {
    if sample.lock_event {
        return Observation::blocked(Reason::Locked);
    }
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
        || matches!(sample.locked, LockFlag::Invalid)
        || sample.protected_data.is_none()
    {
        return Observation::unknown(Reason::ObservationUnavailable);
    }
    if sample.on_console != Some(true) || sample.login_done != Some(true) {
        return Observation::blocked(Reason::InactiveSession);
    }
    if matches!(sample.locked, LockFlag::Boolean(true)) {
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
            locked: LockFlag::Boolean(false),
            lock_event: false,
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
        locked.locked = LockFlag::Boolean(true);
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
                4 => value.locked = LockFlag::Invalid,
                _ => value.protected_data = None,
            }
            assert_eq!(parse(value).state, Availability::Unknown);
        }
    }
    #[test]
    fn absent_spi_flag_is_only_accepted_with_complete_own_console_session() {
        let mut value = valid();
        value.locked = LockFlag::Absent;
        assert_eq!(parse(value).state, Availability::AwakeUnlocked);
        for field in 0..7 {
            let mut value = valid();
            value.locked = LockFlag::Absent;
            match field {
                0 => value.uid = None,
                1 => value.uid = Some(502),
                2 => value.on_console = None,
                3 => value.login_done = None,
                4 => value.protected_data = None,
                5 => value.on_console = Some(false),
                _ => value.login_done = Some(false),
            }
            assert_ne!(parse(value).state, Availability::AwakeUnlocked);
        }
        let mut value = valid();
        value.locked = LockFlag::Absent;
        value.lock_event = true;
        assert_eq!(parse(value).reason, Reason::Locked);
        let mut value = valid();
        value.locked = LockFlag::Absent;
        value.sleeping = true;
        assert_eq!(parse(value).reason, Reason::Sleeping);
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
