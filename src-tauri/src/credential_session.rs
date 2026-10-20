//! Optional, process-owned reuse of credentials already returned by the OS.
//! This is not a Keychain ACL or lock monitor. Never serialize or debug values.
use std::{collections::HashMap, sync::{Arc, Mutex, Weak}};
use zeroize::Zeroizing;

const MAX_KEYS: usize = 64;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_KEY_BYTES: usize = 64 * 1024;
const MAX_ACTIVE_IDS: usize = 64;
const MAX_ID_BYTES: usize = 96;
const CHANGED: &str = "Die Schlüsselsitzung wurde geändert. Starte die Anfrage erneut.";
const LOCKED: &str = "Die Schlüssel sind für diese Sitzung gesperrt. Gib sie unter Modelle erneut frei.";
const UNAVAILABLE: &str = "Die Schlüsselsitzung ist nicht verfügbar.";
const INVALID_ID: &str = "Die Schlüsselreferenz ist ungültig. Wähle einen gespeicherten Zugang erneut aus.";

fn valid_reference(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_BYTES
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub(crate) struct Snapshot {
    pub supported: bool,
    pub enabled: bool,
    pub locked: bool,
    pub remembered_keys: usize,
}

struct State {
    enabled: bool,
    locked: bool,
    epoch: u64,
    bytes: usize,
    values: HashMap<String, Zeroizing<String>>,
    // Weak slots disappear after their final operation/waiter. This also bounds
    // metadata for arbitrary IDs; cached values have their own independent cap.
    active: HashMap<String, Weak<Mutex<()>>>,
}

pub(crate) struct Session {
    supported: bool,
    state: Mutex<State>,
    configuration: Mutex<()>,
}

struct Slot<'a> {
    session: &'a Session,
    id: String,
    lane: Option<Arc<Mutex<()>>>,
    epoch: u64,
}

impl Slot<'_> {
    fn lane(&self) -> &Mutex<()> {
        self.lane.as_deref().expect("live slot retains its lane")
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let Some(lane) = self.lane.take() else { return; };
        let Ok(mut state) = self.session.state.lock() else { return; };
        if Arc::strong_count(&lane) == 1
            && state.active.get(&self.id).is_some_and(|slot| slot.ptr_eq(&Arc::downgrade(&lane))) {
            state.active.remove(&self.id);
        }
        // Serialize the final strong release with slot()'s Weak upgrade and
        // other Slot drops. Leaving this to field/local teardown after the
        // state guard unlocks lets two final drops both observe count == 2.
        drop(lane);
    }
}

impl Session {
    pub fn new(supported: bool, enabled: bool) -> Self {
        Self { supported, state: Mutex::new(State {
            enabled: supported && enabled, locked: false, epoch: 0, bytes: 0,
            values: HashMap::new(), active: HashMap::new(),
        }), configuration: Mutex::new(()) }
    }

    pub fn supported(&self) -> bool { self.supported }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        let state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?;
        Ok(self.snapshot_locked(&state))
    }

    fn snapshot_locked(&self, state: &State) -> Snapshot {
        Snapshot { supported: self.supported, enabled: state.enabled, locked: state.locked, remembered_keys: state.values.len() }
    }

    fn clear_locked(state: &mut State) {
        state.epoch = state.epoch.wrapping_add(1);
        state.values.clear(); // Zeroizing overwrites the retained String buffers.
        state.bytes = 0;
    }

    /// The setting belongs to the installation. Persisting does no OS-secret I/O.
    /// Disabling still clears the live session if its settings write fails.
    pub fn configure(&self, enabled: bool, persist: impl FnOnce() -> Result<(), String>) -> Result<Snapshot, String> {
        if !self.supported { return Err("Sitzungsfreigaben für Schlüssel sind nur unter macOS verfügbar.".into()); }
        let _configuration = self.configuration.lock().map_err(|_| UNAVAILABLE.to_owned())?;
        if !enabled {
            let mut state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?;
            Self::clear_locked(&mut state); state.enabled = false; state.locked = false;
        }
        persist()?;
        let mut state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?;
        if enabled { Self::clear_locked(&mut state); state.enabled = true; state.locked = false; }
        Ok(self.snapshot_locked(&state))
    }

    /// Does not wait for a native password dialog. In-flight reads lose their
    /// epoch and may not return a late value or populate the next session.
    pub fn lock(&self) -> Result<Snapshot, String> {
        if !self.supported { return Err("Sitzungsfreigaben für Schlüssel sind nur unter macOS verfügbar.".into()); }
        let _configuration = self.configuration.lock().map_err(|_| UNAVAILABLE.to_owned())?;
        let mut state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?;
        Self::clear_locked(&mut state);
        state.locked = state.enabled;
        Ok(self.snapshot_locked(&state))
    }

    fn slot(&self, id: &str, reading: bool) -> Result<Slot<'_>, String> {
        if !valid_reference(id) { return Err(INVALID_ID.into()); }
        let mut state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?;
        if reading && state.locked { return Err(LOCKED.into()); }
        state.active.retain(|_, slot| slot.strong_count() != 0);
        let lane = if let Some(lane) = state.active.get(id).and_then(Weak::upgrade) { lane } else {
            if state.active.len() >= MAX_ACTIVE_IDS { return Err("Zu viele gleichzeitige Schlüsselzugriffe. Versuche es nach Abschluss der laufenden Anfragen erneut.".into()); }
            let lane = Arc::new(Mutex::new(()));
            state.active.insert(id.to_owned(), Arc::downgrade(&lane)); lane
        };
        Ok(Slot { session: self, id: id.to_owned(), lane: Some(lane), epoch: state.epoch })
    }

    fn validate(state: &State, epoch: u64) -> Result<(), String> {
        if state.epoch != epoch { return Err(CHANGED.into()); }
        if state.locked { return Err(LOCKED.into()); }
        Ok(())
    }

    fn remove_locked(state: &mut State, id: &str) {
        if let Some(value) = state.values.remove(id) { state.bytes -= value.len(); }
    }

    /// Every caller rechecks its registered ID while owning the same per-ID lane
    /// as write/delete. Never hold the global state mutex during OS access.
    pub fn read(
        &self, id: &str, registered: impl Fn() -> Result<bool, String>,
        load: impl FnOnce() -> Result<Option<String>, String>,
    ) -> Result<Option<String>, String> {
        if !valid_reference(id) { return Err(INVALID_ID.into()); }
        // Reject arbitrary/unregistered references before allocating slot IDs.
        if !registered()? {
            let mut state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?;
            Self::remove_locked(&mut state, id); return Ok(None);
        }
        let slot = self.slot(id, true)?;
        let _operation = slot.lane().lock().map_err(|_| UNAVAILABLE.to_owned())?;
        { let state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?; Self::validate(&state, slot.epoch)?; }
        if !registered()? {
            let mut state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?;
            Self::remove_locked(&mut state, id); return Ok(None);
        }
        {
            let state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?;
            Self::validate(&state, slot.epoch)?;
            if state.enabled { if let Some(value) = state.values.get(id) { return Ok(Some(value.to_string())); } }
        }
        let loaded = load().map(|value| value.map(Zeroizing::new));
        // A denied/missing result is not retained. A late result after Lock or
        // disable is rejected even when the OS operation itself succeeded.
        let mut state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?;
        Self::validate(&state, slot.epoch)?;
        let value = loaded?;
        let Some(value) = value else { return Ok(None); };
        let result = value.to_string();
        if state.enabled && value.len() <= MAX_KEY_BYTES && state.values.len() < MAX_KEYS
            && state.bytes <= MAX_BYTES.saturating_sub(value.len()) {
            state.bytes += value.len(); state.values.insert(id.to_owned(), value);
        }
        Ok(Some(result))
    }

    /// Invalidate before *and* after the persistent mutation, including errors.
    /// Never seed a cache from a write: only a successful authorized read does so.
    pub fn mutate<T>(&self, id: &str, mutation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let slot = self.slot(id, false)?;
        let _operation = slot.lane().lock().map_err(|_| UNAVAILABLE.to_owned())?;
        { let mut state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?; Self::remove_locked(&mut state, id); }
        let result = mutation();
        { let mut state = self.state.lock().map_err(|_| UNAVAILABLE.to_owned())?; Self::remove_locked(&mut state, id); }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::{atomic::{AtomicUsize, Ordering}, mpsc, Barrier}, thread, time::{Duration, Instant}};

    fn wait_for_operations(session: &Session, id: &str, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let observed = session.state.lock().unwrap().active.get(id).map(Weak::strong_count).unwrap_or_default();
            if observed == count { return; }
            assert!(Instant::now() < deadline, "deferred operation did not reach its shared lane");
            thread::yield_now();
        }
    }

    #[test]
    fn off_by_default_and_only_successful_reads_are_reused() {
        let session = Session::new(true, false); let reads = AtomicUsize::new(0);
        let load = || { reads.fetch_add(1, Ordering::SeqCst); Ok(Some("synthetic-value".into())) };
        for _ in 0..2 { session.read("id", || Ok(true), load).unwrap(); }
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        session.configure(true, || Ok(())).unwrap();
        for _ in 0..3 { assert_eq!(session.read("id", || Ok(true), load).unwrap().as_deref(), Some("synthetic-value")); }
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        assert_eq!(session.snapshot().unwrap().remembered_keys, 1);
        let failures = AtomicUsize::new(0);
        for _ in 0..2 {
            assert!(session.read("denied", || Ok(true), || { failures.fetch_add(1, Ordering::SeqCst); Err("synthetic-denial".into()) }).is_err());
            assert_eq!(session.read("missing", || Ok(true), || { failures.fetch_add(1, Ordering::SeqCst); Ok(None) }).unwrap(), None);
        }
        assert_eq!(failures.load(Ordering::SeqCst), 4);
        assert_eq!(session.snapshot().unwrap().remembered_keys, 1);
        assert_eq!(session.read("id", || Ok(false), || panic!("unregistered ID must never read OS")).unwrap(), None);
        assert_eq!(session.snapshot().unwrap().remembered_keys, 0);
    }

    #[test]
    fn concurrent_success_is_single_flight_and_other_ids_are_independent() {
        let session = Arc::new(Session::new(true, true)); let reads = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(5)); let (started, receiving) = mpsc::channel(); let (release, wait) = mpsc::channel();
        let leader = { let session = session.clone(); let reads = reads.clone(); thread::spawn(move || session.read("same", || Ok(true), || {
            reads.fetch_add(1, Ordering::SeqCst); started.send(()).unwrap(); wait.recv().unwrap(); Ok(Some("one-value".into()))
        }).unwrap()) };
        receiving.recv().unwrap();
        let waiters: Vec<_> = (0..4).map(|_| { let session = session.clone(); let reads = reads.clone(); let barrier = barrier.clone(); thread::spawn(move || {
            barrier.wait(); session.read("same", || Ok(true), || { reads.fetch_add(1, Ordering::SeqCst); Ok(Some("duplicate".into())) }).unwrap()
        }) }).collect();
        barrier.wait();
        assert_eq!(session.read("other", || Ok(true), || Ok(Some("independent".into()))).unwrap().as_deref(), Some("independent"));
        release.send(()).unwrap(); assert_eq!(leader.join().unwrap().as_deref(), Some("one-value"));
        for waiter in waiters { assert_eq!(waiter.join().unwrap().as_deref(), Some("one-value")); }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert!(session.state.lock().unwrap().active.is_empty());
    }

    #[test]
    fn simultaneous_slot_teardown_cleans_without_a_later_lookup_and_preserves_live_users() {
        let session = Session::new(true, true);
        // No read/slot call after the joins may prune a leftover dead Weak.
        for _ in 0..16 {
            thread::scope(|scope| {
                let barrier = Arc::new(Barrier::new(3));
                for _ in 0..2 {
                    let slot = session.slot("same", false).unwrap();
                    let barrier = barrier.clone();
                    scope.spawn(move || { barrier.wait(); drop(slot); });
                }
                barrier.wait();
            });
            assert!(session.state.lock().unwrap().active.is_empty());
        }
        let survivor = session.slot("same", false).unwrap();
        thread::scope(|scope| {
            let barrier = Arc::new(Barrier::new(3));
            for _ in 0..2 {
                let slot = session.slot("same", false).unwrap();
                let barrier = barrier.clone();
                scope.spawn(move || { barrier.wait(); drop(slot); });
            }
            barrier.wait();
        });
        assert_eq!(session.state.lock().unwrap().active.get("same").unwrap().strong_count(), 1);
        let successor = session.slot("same", false).unwrap();
        assert!(std::ptr::eq(survivor.lane(), successor.lane()));
        drop(survivor);
        assert_eq!(session.state.lock().unwrap().active.get("same").unwrap().strong_count(), 1);
        drop(successor);
        assert!(session.state.lock().unwrap().active.is_empty());
    }

    #[test]
    fn lock_and_disable_reject_inflight_results_and_lock_blocks_background_reads() {
        for disable in [false, true] {
            let session = Arc::new(Session::new(true, true));
            let (started, receiving) = mpsc::channel(); let (release, wait) = mpsc::channel();
            let reading = { let session = session.clone(); thread::spawn(move || session.read("id", || Ok(true), || {
                started.send(()).unwrap(); wait.recv().unwrap(); Ok(Some("late-value".into()))
            })) };
            receiving.recv().unwrap();
            if disable { session.configure(false, || Ok(())).unwrap(); } else { session.lock().unwrap(); }
            release.send(()).unwrap(); assert_eq!(reading.join().unwrap().unwrap_err(), CHANGED);
            assert_eq!(session.snapshot().unwrap().remembered_keys, 0);
            if !disable { assert_eq!(session.read("id", || Ok(true), || panic!("locked must not touch OS")).unwrap_err(), LOCKED); }
            session.configure(true, || Ok(())).unwrap();
            assert_eq!(session.read("id", || Ok(true), || Ok(Some("new-session".into()))).unwrap().as_deref(), Some("new-session"));
        }
    }

    #[test]
    fn mutations_invalidate_cached_values_on_success_and_failure() {
        let session = Session::new(true, true);
        for failure in [false, true] {
            session.read("id", || Ok(true), || Ok(Some("old".into()))).unwrap();
            let result = session.mutate("id", || if failure { Err("synthetic-write-denial".to_owned()) } else { Ok(()) });
            assert_eq!(result.is_err(), failure); assert_eq!(session.snapshot().unwrap().remembered_keys, 0);
            assert_eq!(session.read("id", || Ok(true), || Ok(Some("fresh".into()))).unwrap().as_deref(), Some("fresh"));
            session.lock().unwrap(); session.configure(true, || Ok(())).unwrap();
        }
    }

    #[test]
    fn read_and_delete_share_the_lane_and_no_stale_value_survives() {
        let session = Arc::new(Session::new(true, true));
        let (started, receiving) = mpsc::channel(); let (release, wait) = mpsc::channel();
        let reading = { let session = session.clone(); thread::spawn(move || session.read("id", || Ok(true), || {
            started.send(()).unwrap(); wait.recv().unwrap(); Ok(Some("old".into()))
        })) };
        receiving.recv().unwrap();
        let mutated = Arc::new(AtomicUsize::new(0));
        let deleting = { let session = session.clone(); let mutated = mutated.clone(); thread::spawn(move || session.mutate("id", || { mutated.fetch_add(1, Ordering::SeqCst); Ok(()) })) };
        wait_for_operations(&session, "id", 2);
        assert_eq!(mutated.load(Ordering::SeqCst), 0);
        release.send(()).unwrap(); reading.join().unwrap().unwrap(); deleting.join().unwrap().unwrap();
        assert_eq!(mutated.load(Ordering::SeqCst), 1);
        assert_eq!(session.snapshot().unwrap().remembered_keys, 0);
        assert_eq!(session.read("id", || Ok(false), || panic!("deleted registry reference")).unwrap(), None);
    }

    #[test]
    fn lock_then_resume_rejects_the_old_reader_and_queued_waiter_without_blocking_the_new_session() {
        let session = Arc::new(Session::new(true, true));
        let (started, receiving) = mpsc::channel(); let (release, wait) = mpsc::channel();
        let reading = { let session = session.clone(); thread::spawn(move || session.read("id", || Ok(true), || {
            started.send(()).unwrap(); wait.recv().unwrap(); Ok(Some("old-session-value".into()))
        })) };
        receiving.recv().unwrap();
        let queued = { let session = session.clone(); thread::spawn(move || session.read("id", || Ok(true), || panic!("old waiter must not enter OS after resume"))) };
        wait_for_operations(&session, "id", 2);
        session.lock().unwrap(); session.configure(true, || Ok(())).unwrap();
        release.send(()).unwrap();
        assert_eq!(reading.join().unwrap().unwrap_err(), CHANGED);
        assert_eq!(queued.join().unwrap().unwrap_err(), CHANGED);
        assert!(session.state.lock().unwrap().active.is_empty());
        assert_eq!(session.read("id", || Ok(true), || Ok(Some("new-session-value".into()))).unwrap().as_deref(), Some("new-session-value"));
    }

    #[test]
    fn limits_bound_values_bytes_and_active_slots_without_altering_loaded_values() {
        let session = Session::new(true, true);
        let large = "x".repeat(MAX_KEY_BYTES + 1);
        for _ in 0..2 { assert_eq!(session.read("large", || Ok(true), || Ok(Some(large.clone()))).unwrap().as_deref(), Some(large.as_str())); }
        assert_eq!(session.snapshot().unwrap().remembered_keys, 0);
        for id in 0..MAX_KEYS + 10 { session.read(&id.to_string(), || Ok(true), || Ok(Some("small".into()))).unwrap(); }
        assert_eq!(session.snapshot().unwrap().remembered_keys, MAX_KEYS);
        assert!(session.state.lock().unwrap().active.is_empty());
        session.lock().unwrap(); session.configure(true, || Ok(())).unwrap();
        for id in 0..MAX_KEYS { session.read(&id.to_string(), || Ok(true), || Ok(Some("x".repeat(MAX_KEY_BYTES)))).unwrap(); }
        assert_eq!(session.state.lock().unwrap().bytes, MAX_BYTES);
        let slots: Vec<_> = (0..MAX_ACTIVE_IDS).map(|id| session.slot(&id.to_string(), false).unwrap()).collect();
        assert!(session.slot("overflow", false).is_err());
        drop(slots); assert!(session.state.lock().unwrap().active.is_empty());
        assert!(session.slot("available-again", false).is_ok());
    }

    #[test]
    fn failed_setting_write_cannot_keep_disabled_copies_or_enable_reuse() {
        let session = Session::new(true, true); session.read("id", || Ok(true), || Ok(Some("value".into()))).unwrap();
        assert!(session.configure(false, || Err("synthetic-db-error".into())).is_err());
        let status = session.snapshot().unwrap(); assert!(!status.enabled); assert_eq!(status.remembered_keys, 0);
        assert!(session.configure(true, || Err("synthetic-db-error".into())).is_err());
        assert!(!session.snapshot().unwrap().enabled);
        assert!(Session::new(false, false).configure(true, || panic!("unsupported must not persist")).is_err());
    }

    #[test]
    fn malformed_references_never_allocate_slots_or_reach_the_registry_or_backend() {
        let session = Session::new(true, true);
        for id in ["".to_owned(), "a".repeat(MAX_ID_BYTES + 1), "a".repeat(1024 * 1024), "key/name".into(), "key with spaces".into(), "clé".into()] {
            assert_eq!(session.read(&id, || panic!("invalid reference before registry"), || panic!("invalid reference before OS")).unwrap_err(), INVALID_ID);
            assert!(session.mutate::<()>(&id, || panic!("invalid reference before mutation")).is_err());
        }
        assert!(session.state.lock().unwrap().active.is_empty());
        assert_eq!(session.snapshot().unwrap().remembered_keys, 0);
        assert!(session.read(&"a".repeat(MAX_ID_BYTES), || Ok(true), || Ok(Some("boundary".into()))).is_ok());
    }
}
