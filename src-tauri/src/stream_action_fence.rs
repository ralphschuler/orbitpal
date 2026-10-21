//! Process-only stream provenance retained across native awaits and SQL waits.
//! No Core/Store/GUI acquisition occurs while checking an admitted action.
use super::DispatchLease;
use crate::{
    chat_route::ImpulseSource,
    storage::{
        action_store::{self, ProfileToken},
        Store,
    },
};
use rusqlite::Connection;
use serde_json::Value;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const ERROR: &str = "stream_action_source_unavailable";
const MAX_LIFETIME_MS: u64 = 30_000;

#[derive(Clone)]
pub(super) struct NativeDeadline {
    until: Instant,
    #[cfg(test)]
    controlled: Option<(Arc<std::sync::atomic::AtomicU64>, u64)>,
}
impl NativeDeadline {
    pub(super) fn from_wall(expires: u64, wall: u64, sampled_at: Instant) -> Option<Self> {
        if expires > crate::experience::MAX_TIME || wall > crate::experience::MAX_TIME {
            return None;
        }
        let remaining = expires
            .checked_sub(wall)
            .filter(|value| *value > 0 && *value <= MAX_LIFETIME_MS)?;
        let value = Self {
            until: sampled_at.checked_add(Duration::from_millis(remaining))?,
            #[cfg(test)]
            controlled: None,
        };
        value.current().then_some(value)
    }
    pub(super) fn current(&self) -> bool {
        #[cfg(test)]
        if let Some((clock, until)) = &self.controlled {
            return clock.load(Ordering::SeqCst) < *until;
        }
        Instant::now() < self.until
    }
}

/// Neither deserializable nor an action/permission factory. The native receive
/// lease freezes its owner, effective profile and exact raw SQLite profile token.
#[derive(Clone)]
pub(crate) struct StreamActionFence {
    source: ImpulseSource,
    deadline: NativeDeadline,
    session: String,
    profile: Value,
    token: ProfileToken,
}
impl StreamActionFence {
    pub(super) fn capture(lease: &DispatchLease, store: &Store) -> Result<Self, String> {
        if !lease.deadline.current() {
            return Err(ERROR.into());
        }
        let profile = store.bot_profile(lease.owner())?;
        if !lease.source.current_profile(&profile) {
            return Err(ERROR.into());
        }
        let token = store
            .action_profile_token(lease.owner(), &profile)
            .map_err(|_| ERROR)?;
        let fence = Self {
            source: lease.source.clone(),
            deadline: lease.deadline.clone(),
            session: lease.session_id.clone(),
            profile,
            token,
        };
        if !fence.current() {
            return Err(ERROR.into());
        }
        Ok(fence)
    }
    pub(crate) fn owner(&self) -> &str {
        self.source.owner()
    }
    pub(crate) fn session_id(&self) -> &str {
        &self.session
    }
    pub(crate) fn current(&self) -> bool {
        self.deadline.current()
            && self.source.current_profile(&self.profile)
            && self.deadline.current()
    }
    pub(crate) fn transaction_current(&self, db: &Connection) -> bool {
        self.current()
            && action_store::profile_current(db, self.owner(), &self.token)
            && self.current()
    }
    pub(crate) fn bind_cancel(&self, job: &Arc<AtomicBool>) -> Result<(), String> {
        if !self.current() {
            job.store(true, Ordering::SeqCst);
            return Err(ERROR.into());
        }
        self.source.bind_cancel(job)?;
        if !self.current() {
            job.store(true, Ordering::SeqCst);
            return Err(ERROR.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_route::ConnectionContext;
    use std::{path::PathBuf, sync::atomic::AtomicU64};

    struct Fixture {
        root: PathBuf,
        store: Option<Store>,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("orbitpal-stream-fence-{}", uuid::Uuid::new_v4()));
            let store = Store::new(root.clone()).unwrap();
            Self {
                root,
                store: Some(store),
            }
        }
        fn store(&self) -> &Store {
            self.store.as_ref().unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(self.store.take());
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn lease(store: &Store, clock: Arc<AtomicU64>) -> (DispatchLease, ConnectionContext) {
        let profile = store.profile().unwrap();
        let mut context =
            ConnectionContext::new("twitch", &profile, None, Arc::new(AtomicBool::new(false)))
                .unwrap();
        context.mark_connected("controlled-native-channel");
        let source = context
            .impulse_source("twitch", context.route_id().unwrap(), &profile)
            .unwrap();
        (
            DispatchLease {
                source,
                deadline: NativeDeadline {
                    until: Instant::now(),
                    controlled: Some((clock, 100)),
                },
                session_id: "actual-test-session".into(),
            },
            context,
        )
    }
    #[test]
    fn frozen_owner_token_is_checked_in_the_callers_actual_sqlite_transaction() {
        let fixture = Fixture::new();
        let clock = Arc::new(AtomicU64::new(0));
        let (lease, _context) = lease(fixture.store(), clock);
        let fence = lease.action_fence(fixture.store()).unwrap();
        assert_eq!(
            fence.owner(),
            fixture.store().profile().unwrap()["id"].as_str().unwrap()
        );
        assert_eq!(fence.session_id(), "actual-test-session");
        let partner = fixture.store().create_bot("Partner", false).unwrap();
        let mut db = Connection::open(fixture.root.join("cohost.sqlite3")).unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(fence.transaction_current(&tx));
        // The owner is deliberately different from the acting scene partner.
        let mut partner = partner;
        partner["name"] = serde_json::json!("Changed partner");
        tx.execute(
            "UPDATE bot_profiles SET profile=?2 WHERE id=?1",
            rusqlite::params![partner["id"].as_str().unwrap(), partner.to_string()],
        )
        .unwrap();
        assert!(
            fence.transaction_current(&tx),
            "a partner token cannot replace the source owner's token"
        );
        let raw: String = tx
            .query_row(
                "SELECT profile FROM bot_profiles WHERE id=?1",
                [fence.owner()],
                |row| row.get(0),
            )
            .unwrap();
        let mut owner: Value = serde_json::from_str(&raw).unwrap();
        owner["name"] = serde_json::json!("Changed owner");
        tx.execute(
            "UPDATE bot_profiles SET profile=?2 WHERE id=?1",
            rusqlite::params![fence.owner(), owner.to_string()],
        )
        .unwrap();
        assert!(
            fence.current(),
            "pure source checks cannot reread the Store"
        );
        assert!(
            !fence.transaction_current(&tx),
            "raw owner changes inside this very TX must be observed"
        );
    }
    #[test]
    fn deadline_elapsed_while_waiting_for_sql_cannot_start_or_complete_work() {
        let fixture = Fixture::new();
        let clock = Arc::new(AtomicU64::new(0));
        let (lease, _context) = lease(fixture.store(), clock.clone());
        let fence = lease.action_fence(fixture.store()).unwrap();
        let mut db = Connection::open(fixture.root.join("cohost.sqlite3")).unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        clock.store(99, Ordering::SeqCst);
        assert!(fence.transaction_current(&tx));
        clock.store(100, Ordering::SeqCst);
        assert!(!fence.current());
        assert!(!fence.transaction_current(&tx));
        assert!(lease.action_fence(fixture.store()).is_err());
        let job = Arc::new(AtomicBool::new(false));
        assert!(fence.bind_cancel(&job).is_err());
        assert!(job.load(Ordering::SeqCst));
    }
    #[test]
    fn actual_source_disconnect_revokes_both_scene_partner_job_tokens_and_sql_fence() {
        let fixture = Fixture::new();
        let clock = Arc::new(AtomicU64::new(0));
        let (lease, mut context) = lease(fixture.store(), clock);
        let fence = lease.action_fence(fixture.store()).unwrap();
        let first = Arc::new(AtomicBool::new(false));
        let second = Arc::new(AtomicBool::new(false));
        lease.bind_cancel(&first).unwrap();
        fence.bind_cancel(&first).unwrap();
        fence.bind_cancel(&second).unwrap();
        let db = Connection::open(fixture.root.join("cohost.sqlite3")).unwrap();
        assert!(fence.transaction_current(&db));
        context.disconnected();
        assert!(first.load(Ordering::SeqCst));
        assert!(second.load(Ordering::SeqCst));
        assert!(!fence.current());
        assert!(!fence.transaction_current(&db));
    }
    #[test]
    fn native_deadline_counts_time_before_capture_and_rejects_unknown_lifetimes() {
        let sampled = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        assert!(NativeDeadline::from_wall(2000, 1000, sampled).is_none());
        for expiry in [999, 1000, 31001] {
            assert!(NativeDeadline::from_wall(expiry, 1000, Instant::now()).is_none());
        }
        assert!(NativeDeadline::from_wall(31000, 1000, Instant::now())
            .unwrap()
            .current());
    }
}
