//! Read-only current-process login1 monitor. A concrete session and unique
//! service owner are pinned for each connection; no /session/self signal alias,
//! user/session enumeration, screen data or session-changing call is used.
use super::{
    linux_state::{self, Event, SessionSample},
    AvailabilityGate, Observation, Reason,
};
use futures_util::{stream::SelectAll, StreamExt};
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, Notify};
use zbus::{
    names::OwnedUniqueName, proxy::CacheProperties, Connection, MatchRule, Message, MessageStream,
    OwnedObjectPath, OwnedVariant, Proxy,
};

const SERVICE: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const SESSION: &str = "org.freedesktop.login1.Session";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_MESSAGE: usize = 16_384;
type Request = oneshot::Sender<Observation>;

struct Control {
    gate: Arc<AvailabilityGate>,
    stopped: AtomicBool,
    stop: Notify,
}
pub(super) struct Monitor {
    control: Arc<Control>,
    requests: mpsc::Sender<Request>,
    _task: tokio::task::JoinHandle<()>,
}
struct Session {
    connection: Connection,
    owner: OwnedUniqueName,
    path: OwnedObjectPath,
    events: SelectAll<MessageStream>,
}

impl Monitor {
    pub(super) fn start(_app: tauri::AppHandle, gate: Arc<AvailabilityGate>) -> Self {
        let control = Arc::new(Control {
            gate,
            stopped: AtomicBool::new(false),
            stop: Notify::new(),
        });
        let (requests, receiver) = mpsc::channel(8);
        let task = tokio::spawn(monitor(control.clone(), receiver));
        Self {
            control,
            requests,
            _task: task,
        }
    }
    pub(super) async fn sample(&self) -> Observation {
        if self.control.stopped.load(Ordering::SeqCst) {
            return disconnected();
        }
        let (sender, receiver) = oneshot::channel();
        if self.requests.try_send(sender).is_err() {
            return Observation::unknown(Reason::ObservationUnavailable);
        }
        tokio::time::timeout(QUERY_TIMEOUT, receiver)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_else(|| Observation::unknown(Reason::ObservationUnavailable))
    }
    pub(super) fn stop(&self) {
        self.control.stopped.store(true, Ordering::SeqCst);
        self.control.gate.block(Reason::MonitorDisconnected);
        self.control.stop.notify_one();
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop();
    }
}
fn disconnected() -> Observation {
    Observation::unknown(Reason::MonitorDisconnected)
}

async fn monitor(control: Arc<Control>, requests: mpsc::Receiver<Request>) {
    monitor_using(control, requests, || {
        zbus::connection::Builder::system()
            .method_timeout(Duration::from_secs(1))
            .max_queued(NonZeroUsize::new(32).unwrap())
            .build()
    })
    .await;
}
async fn monitor_using<F, Fut>(
    control: Arc<Control>,
    mut requests: mpsc::Receiver<Request>,
    connect: F,
) where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = zbus::Result<Connection>>,
{
    let mut retry = 1u64;
    while !control.stopped.load(Ordering::SeqCst) {
        control.gate.block(Reason::Initializing);
        let connection = tokio::select! { biased;
            _=control.stop.notified()=>break,
            result=tokio::time::timeout(QUERY_TIMEOUT,connect())=>result.ok().and_then(Result::ok),
        };
        if let Some(connection) = connection {
            let setup = tokio::select! { biased;
                _=control.stop.notified()=>None,
                result=tokio::time::timeout(QUERY_TIMEOUT,subscribe(connection.clone()))=>result.ok().and_then(Result::ok),
            };
            if let Some(mut session) = setup {
                retry = 1;
                serve(&control, &mut requests, &mut session).await;
                // Dropping every stream and closing this owned connection also
                // removes bus match rules. No detached listener is retained.
                drop(session);
            }
            let _ = tokio::time::timeout(Duration::from_millis(250), connection.close()).await;
        }
        control.gate.block(Reason::MonitorDisconnected);
        while let Ok(reply) = requests.try_recv() {
            let _ = reply.send(disconnected());
        }
        if control.stopped.load(Ordering::SeqCst) {
            break;
        }
        tokio::select! { biased; _=control.stop.notified()=>break, _=tokio::time::sleep(Duration::from_secs(retry))=>{} }
        retry = (retry * 2).min(30);
    }
    control.gate.block(Reason::MonitorDisconnected);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life_availability::{Availability, Source};
    use std::sync::{atomic::AtomicUsize, Mutex};
    const OWNER: &str = ":1.77";
    const PATH: &str = "/org/freedesktop/login1/session/c7";
    struct Model {
        locked: AtomicBool,
        active: AtomicBool,
        sleeping: AtomicBool,
        hold: AtomicBool,
        state: Mutex<String>,
        path: Mutex<String>,
        owner: Mutex<String>,
        reads: AtomicUsize,
        reading: Notify,
        release: Notify,
    }
    impl Default for Model {
        fn default() -> Self {
            Self {
                locked: AtomicBool::new(false),
                active: AtomicBool::new(true),
                sleeping: AtomicBool::new(false),
                hold: AtomicBool::new(false),
                state: Mutex::new("active".into()),
                path: Mutex::new(PATH.into()),
                owner: Mutex::new(OWNER.into()),
                reads: AtomicUsize::new(0),
                reading: Notify::new(),
                release: Notify::new(),
            }
        }
    }
    struct FakeBus(Arc<Model>);
    #[zbus::interface(name = "org.freedesktop.DBus")]
    impl FakeBus {
        fn get_name_owner(&self, name: &str) -> zbus::fdo::Result<OwnedUniqueName> {
            if name != SERVICE {
                return Err(zbus::fdo::Error::NameHasNoOwner(
                    "Synthetic missing owner".into(),
                ));
            }
            Ok(self.0.owner.lock().unwrap().as_str().try_into().unwrap())
        }
    }
    struct FakeManager(Arc<Model>);
    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl FakeManager {
        #[zbus(name = "GetSessionByPID")]
        fn get_session_by_pid(&self, pid: u32) -> zbus::fdo::Result<OwnedObjectPath> {
            if pid != std::process::id() {
                return Err(zbus::fdo::Error::Failed("Wrong synthetic PID".into()));
            }
            Ok(self.0.path.lock().unwrap().as_str().try_into().unwrap())
        }
        #[zbus(property)]
        fn preparing_for_sleep(&self) -> bool {
            self.0.sleeping.load(Ordering::SeqCst)
        }
    }
    struct FakeSession(Arc<Model>);
    #[zbus::interface(name = "org.freedesktop.login1.Session")]
    impl FakeSession {
        #[zbus(property)]
        async fn locked_hint(&self) -> bool {
            self.0.reads.fetch_add(1, Ordering::SeqCst);
            let locked = self.0.locked.load(Ordering::SeqCst);
            if self.0.hold.swap(false, Ordering::SeqCst) {
                self.0.reading.notify_one();
                self.0.release.notified().await;
            }
            locked
        }
        #[zbus(property)]
        fn active(&self) -> bool {
            self.0.active.load(Ordering::SeqCst)
        }
        #[zbus(property)]
        fn state(&self) -> String {
            self.0.state.lock().unwrap().clone()
        }
    }
    async fn peer(model: Arc<Model>) -> (Connection, Connection) {
        let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        let server = zbus::connection::Builder::unix_stream(server)
            .p2p()
            .server(zbus::Guid::generate())
            .serve_at(BUS_PATH, FakeBus(model.clone()))
            .serve_at(MANAGER_PATH, FakeManager(model.clone()))
            .serve_at(PATH, FakeSession(model))
            .build();
        let client = zbus::connection::Builder::unix_stream(client)
            .p2p()
            .unique_name(":1.78")
            .method_timeout(Duration::from_millis(500))
            .build();
        let (server, client) = tokio::try_join!(server, client).unwrap();
        (client, server)
    }
    async fn finish_peer(server: Connection) {
        // The counterpart was deliberately closed first. macOS reports
        // ENOTCONN for a second shutdown of that already disconnected socket.
        match server.close().await {
            Ok(()) => {},
            Err(zbus::Error::InputOutput(error)) if error.kind() == std::io::ErrorKind::NotConnected => {},
            Err(error) => panic!("Synthetic peer cleanup failed: {error}"),
        }
    }
    fn control() -> Arc<Control> {
        Arc::new(Control {
            gate: Arc::new(AvailabilityGate::new(Source::LinuxLogind)),
            stopped: AtomicBool::new(false),
            stop: Notify::new(),
        })
    }
    fn properties(
        owner: &str,
        path: &str,
        changed: HashMap<String, OwnedVariant>,
        invalidated: Vec<String>,
    ) -> Message {
        Message::signal(path, PROPERTIES, "PropertiesChanged")
            .sender(owner)
            .build(&(SESSION, changed, invalidated))
            .unwrap()
    }
    #[test]
    fn exact_signal_types_paths_owners_and_invalidation_are_enforced() {
        let clear = properties(
            OWNER,
            PATH,
            HashMap::from([("LockedHint".into(), false.into())]),
            vec![],
        );
        assert_eq!(
            event_effect(&clear, OWNER, PATH).reason,
            Some(Reason::Initializing)
        );
        let locked = properties(
            OWNER,
            PATH,
            HashMap::from([("LockedHint".into(), true.into())]),
            vec![],
        );
        assert_eq!(
            event_effect(&locked, OWNER, PATH).reason,
            Some(Reason::Locked)
        );
        assert!(event_effect(&locked, ":1.99", PATH).reason.is_none());
        assert!(
            event_effect(&locked, OWNER, "/org/freedesktop/login1/session/other")
                .reason
                .is_none()
        );
        let wrong = properties(
            OWNER,
            PATH,
            HashMap::from([("LockedHint".into(), 1u32.into())]),
            vec![],
        );
        assert_eq!(
            event_effect(&wrong, OWNER, PATH).reason,
            Some(Reason::ObservationUnavailable)
        );
        let invalidated = properties(OWNER, PATH, HashMap::new(), vec!["Active".into()]);
        assert_eq!(
            event_effect(&invalidated, OWNER, PATH).reason,
            Some(Reason::ObservationUnavailable)
        );
        let unrelated = properties(
            OWNER,
            PATH,
            HashMap::from([("IdleHint".into(), true.into())]),
            vec![],
        );
        assert!(event_effect(&unrelated, OWNER, PATH).reason.is_none());
        let malformed = Message::signal(PATH, PROPERTIES, "PropertiesChanged")
            .sender(OWNER)
            .build(&true)
            .unwrap();
        assert_eq!(
            event_effect(&malformed, OWNER, PATH).reason,
            Some(Reason::ObservationUnavailable)
        );
        let excessive = properties(OWNER, PATH, HashMap::new(), vec!["Active".into(); 33]);
        assert_eq!(
            event_effect(&excessive, OWNER, PATH).reason,
            Some(Reason::ObservationUnavailable)
        );
        let large = properties(
            OWNER,
            PATH,
            HashMap::from([(
                "State".into(),
                OwnedVariant::from(zbus::Str::from("x".repeat(MAX_MESSAGE))),
            )]),
            vec![],
        );
        assert_eq!(
            event_effect(&large, OWNER, PATH).reason,
            Some(Reason::ObservationUnavailable)
        );
    }
    #[test]
    fn sleep_clear_owner_loss_and_session_removal_never_grant() {
        for (value, reason) in [(true, Reason::Sleeping), (false, Reason::Initializing)] {
            let message = Message::signal(MANAGER_PATH, MANAGER, "PrepareForSleep")
                .sender(OWNER)
                .build(&value)
                .unwrap();
            assert_eq!(event_effect(&message, OWNER, PATH).reason, Some(reason));
        }
        let lost = Message::signal(BUS_PATH, BUS, "NameOwnerChanged")
            .sender(BUS)
            .build(&(SERVICE, OWNER, ""))
            .unwrap();
        assert!(event_effect(&lost, OWNER, PATH).reconnect);
        let removed = Message::signal(MANAGER_PATH, MANAGER, "SessionRemoved")
            .sender(OWNER)
            .build(&("synthetic", OwnedObjectPath::try_from(PATH).unwrap()))
            .unwrap();
        assert!(event_effect(&removed, OWNER, PATH).reconnect);
        assert!(!event_effect(&removed, OWNER, "/org/freedesktop/login1/session/other").reconnect);
    }
    #[tokio::test]
    async fn concrete_current_pid_observers_precede_initial_read_and_properties_are_fresh() {
        let model = Arc::new(Model::default());
        let (connection, server) = peer(model.clone()).await;
        let mut session = subscribe(connection.clone()).await.unwrap();
        assert_eq!(session.path.as_str(), PATH);
        assert_eq!(session.owner.as_str(), OWNER);
        assert_eq!(session.events.len(), 4);
        assert_eq!(model.reads.load(Ordering::SeqCst), 0);
        let control = control();
        assert_eq!(query(&control, &mut session).await.0, Observation::ready());
        model.locked.store(true, Ordering::SeqCst);
        assert_eq!(query(&control, &mut session).await.0.reason, Reason::Locked);
        model.locked.store(false, Ordering::SeqCst);
        model.active.store(false, Ordering::SeqCst);
        assert_eq!(
            query(&control, &mut session).await.0.reason,
            Reason::InactiveSession
        );
        model.active.store(true, Ordering::SeqCst);
        model.sleeping.store(true, Ordering::SeqCst);
        assert_eq!(
            query(&control, &mut session).await.0.reason,
            Reason::Sleeping
        );
        model.sleeping.store(false, Ordering::SeqCst);
        *model.state.lock().unwrap() = "future-state".into();
        assert_eq!(
            query(&control, &mut session).await.0.state,
            Availability::Unknown
        );
        connection.close().await.unwrap();
        finish_peer(server).await;
    }
    #[tokio::test]
    async fn event_during_delayed_clear_read_fences_it_before_commit() {
        let model = Arc::new(Model::default());
        let (connection, server) = peer(model.clone()).await;
        let mut session = subscribe(connection.clone()).await.unwrap();
        let control = control();
        model.hold.store(true, Ordering::SeqCst);
        let querying = {
            let control = control.clone();
            tokio::spawn(async move {
                let result = query(&control, &mut session).await;
                (result, session)
            })
        };
        model.reading.notified().await;
        let message = properties(
            OWNER,
            PATH,
            HashMap::from([("LockedHint".into(), true.into())]),
            vec![],
        );
        server.send(&message).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while control.gate.snapshot().reason != Reason::Locked {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        model.release.notify_one();
        let ((_, reconnect, resample), _session) = querying.await.unwrap();
        assert!(!reconnect);
        assert!(resample);
        assert_eq!(control.gate.snapshot().reason, Reason::Locked);
        connection.close().await.unwrap();
        finish_peer(server).await;
    }
    #[tokio::test]
    async fn clear_requires_new_sample_and_owner_loss_cancels_pending_read() {
        let model = Arc::new(Model::default());
        let (connection, server) = peer(model.clone()).await;
        let mut session = subscribe(connection.clone()).await.unwrap();
        let control = control();
        control.gate.block(Reason::Locked);
        model.hold.store(true, Ordering::SeqCst);
        let querying = {
            let control = control.clone();
            tokio::spawn(async move {
                let result = query(&control, &mut session).await;
                (result, session)
            })
        };
        model.reading.notified().await;
        server
            .send(&properties(
                OWNER,
                PATH,
                HashMap::from([("LockedHint".into(), false.into())]),
                vec![],
            ))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while control.gate.snapshot().reason != Reason::Initializing {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(control.gate.snapshot().state, Availability::Unknown);
        model.release.notify_one();
        let (_, mut session) = querying.await.unwrap();
        assert_eq!(control.gate.snapshot().state, Availability::Unknown);
        assert_eq!(query(&control, &mut session).await.0, Observation::ready());
        assert_eq!(control.gate.snapshot().state, Availability::AwakeUnlocked);
        let cancelled = Arc::new(AtomicBool::new(false));
        control.gate.watch(&cancelled).unwrap();
        model.hold.store(true, Ordering::SeqCst);
        let querying = {
            let control = control.clone();
            tokio::spawn(async move { query(&control, &mut session).await })
        };
        model.reading.notified().await;
        let lost = Message::signal(BUS_PATH, BUS, "NameOwnerChanged")
            .sender(BUS)
            .build(&(SERVICE, OWNER, ""))
            .unwrap();
        server.send(&lost).await.unwrap();
        let (_, reconnect, _) = tokio::time::timeout(Duration::from_secs(1), querying)
            .await
            .unwrap()
            .unwrap();
        assert!(reconnect);
        assert!(cancelled.load(Ordering::SeqCst));
        assert_eq!(control.gate.snapshot().reason, Reason::MonitorDisconnected);
        model.release.notify_one();
        tokio::task::yield_now().await;
        assert_eq!(control.gate.snapshot().state, Availability::Unknown);
        let _ = connection.close().await;
        let _ = server.close().await;
    }
    #[tokio::test]
    async fn changed_current_process_path_owner_loss_and_transport_disconnect_fail_closed() {
        let model = Arc::new(Model::default());
        let (connection, server) = peer(model.clone()).await;
        let mut session = subscribe(connection.clone()).await.unwrap();
        let control = control();
        query(&control, &mut session).await;
        assert_eq!(control.gate.snapshot().state, Availability::AwakeUnlocked);
        *model.path.lock().unwrap() = "/org/freedesktop/login1/session/other".into();
        assert_eq!(
            query(&control, &mut session).await.0.reason,
            Reason::MonitorDisconnected
        );
        *model.path.lock().unwrap() = PATH.into();
        *model.owner.lock().unwrap() = ":1.99".into();
        assert_eq!(
            query(&control, &mut session).await.0.reason,
            Reason::MonitorDisconnected
        );
        *model.owner.lock().unwrap() = OWNER.into();
        let lost = Message::signal(BUS_PATH, BUS, "NameOwnerChanged")
            .sender(BUS)
            .build(&(SERVICE, OWNER, ""))
            .unwrap();
        assert!(observe(&control, Some(Ok(lost)), OWNER, PATH).reconnect);
        assert_eq!(control.gate.snapshot().reason, Reason::MonitorDisconnected);
        finish_peer(server).await;
        assert!(query(&control, &mut session).await.1);
        let _ = connection.close().await;
    }
    #[tokio::test]
    async fn stop_cancels_cloud_watchers_and_closes_owned_connection() {
        let model = Arc::new(Model::default());
        let (connection, server) = peer(model).await;
        let control = control();
        let (send, receive) = mpsc::channel(8);
        let worker = {
            let control = control.clone();
            let connection = connection.clone();
            tokio::spawn(monitor_using(control, receive, move || {
                let connection = connection.clone();
                async move { Ok(connection) }
            }))
        };
        let mut monitor = Monitor {
            control: control.clone(),
            requests: send,
            _task: worker,
        };
        tokio::time::timeout(Duration::from_secs(1), async {
            while control.gate.snapshot().state != Availability::AwakeUnlocked {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        assert!(control.gate.watch(&cancelled).is_some());
        monitor.stop();
        assert!(cancelled.load(Ordering::SeqCst));
        assert_eq!(monitor.sample().await.reason, Reason::MonitorDisconnected);
        tokio::time::timeout(Duration::from_secs(1), &mut monitor._task)
            .await
            .unwrap()
            .unwrap();
        assert!(cancelled.load(Ordering::SeqCst));
        assert_eq!(control.gate.snapshot().reason, Reason::MonitorDisconnected);
        tokio::time::timeout(Duration::from_secs(1), connection.closed())
            .await
            .unwrap();
        finish_peer(server).await;
    }
    #[tokio::test]
    async fn failed_connection_retries_are_paced_and_stop_interrupts_backoff() {
        let control = control();
        let (_send, receive) = mpsc::channel(8);
        let attempts = Arc::new(AtomicUsize::new(0));
        let worker = {
            let control = control.clone();
            let attempts = attempts.clone();
            tokio::spawn(monitor_using(control, receive, move || {
                attempts.fetch_add(1, Ordering::SeqCst);
                async { Err(zbus::Error::Failure("Synthetic unavailable".into())) }
            }))
        };
        tokio::time::timeout(Duration::from_secs(2), async {
            while attempts.load(Ordering::SeqCst) < 2 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert_eq!(control.gate.snapshot().reason, Reason::MonitorDisconnected);
        control.stopped.store(true, Ordering::SeqCst);
        control.stop.notify_one();
        tokio::time::timeout(Duration::from_millis(500), worker)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }
}

async fn proxy<'a>(
    connection: &Connection,
    owner: &'a str,
    path: &'a str,
    interface: &'a str,
) -> zbus::Result<Proxy<'a>> {
    zbus::proxy::Builder::new(connection)
        .destination(owner)
        .path(path)
        .interface(interface)
        .cache_properties(CacheProperties::No)
        .build()
        .await
}
async fn owner(connection: &Connection) -> zbus::Result<OwnedUniqueName> {
    zbus::fdo::DBusProxy::new(connection)
        .await?
        .get_name_owner(SERVICE.try_into().unwrap())
        .await
        .map_err(Into::into)
}
async fn session_path(manager: &Proxy<'_>) -> zbus::Result<OwnedObjectPath> {
    manager
        .call("GetSessionByPID", &(std::process::id(),))
        .await
}
async fn stream(
    connection: &Connection,
    sender: &str,
    path: &str,
    interface: &str,
    member: &str,
) -> zbus::Result<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(sender)
        .path(path)
        .interface(interface)
        .member(member)
        .build()?;
    MessageStream::for_match_rule(rule, connection, NonZeroUsize::new(16)).await
}
async fn subscribe(connection: Connection) -> zbus::Result<Session> {
    let mut events = SelectAll::new();
    // Subscribe to owner changes before resolving the current owner/session.
    // An activation/restart during setup stays queued and invalidates sampling.
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(BUS)
        .path(BUS_PATH)
        .interface(BUS)
        .member("NameOwnerChanged")
        .arg(0, SERVICE)
        .build()?;
    events.push(MessageStream::for_match_rule(rule, &connection, NonZeroUsize::new(16)).await?);
    let owner = owner(&connection).await?;
    let manager = proxy(&connection, owner.as_str(), MANAGER_PATH, MANAGER).await?;
    let path = session_path(&manager).await?;
    if !linux_state::concrete_session_path(path.as_str()) {
        return Err(zbus::Error::Failure("Availability unavailable".into()));
    }
    events.push(
        stream(
            &connection,
            owner.as_str(),
            path.as_str(),
            PROPERTIES,
            "PropertiesChanged",
        )
        .await?,
    );
    events.push(
        stream(
            &connection,
            owner.as_str(),
            MANAGER_PATH,
            MANAGER,
            "PrepareForSleep",
        )
        .await?,
    );
    events.push(
        stream(
            &connection,
            owner.as_str(),
            MANAGER_PATH,
            MANAGER,
            "SessionRemoved",
        )
        .await?,
    );
    if self::owner(&connection).await? != owner || session_path(&manager).await? != path {
        return Err(zbus::Error::Failure("Availability unavailable".into()));
    }
    drop(manager);
    Ok(Session {
        connection,
        owner,
        path,
        events,
    })
}

async fn read_sample(
    connection: &Connection,
    expected_owner: &str,
    expected_path: &str,
) -> Observation {
    let result = async {
        let manager = proxy(connection, expected_owner, MANAGER_PATH, MANAGER).await?;
        let session = proxy(connection, expected_owner, expected_path, SESSION).await?;
        if session_path(&manager).await?.as_str() != expected_path {
            return Ok(disconnected());
        }
        // These four typed uncached reads expose no names, IDs, desktop images
        // or other session metadata. Unknown/missing/wrong types fail closed.
        let (locked, active, state, sleeping): (bool, bool, String, bool) = tokio::try_join!(
            session.get_property("LockedHint"),
            session.get_property("Active"),
            session.get_property("State"),
            manager.get_property("PreparingForSleep")
        )?;
        if state.len() > 32
            || owner(connection).await?.as_str() != expected_owner
            || session_path(&manager).await?.as_str() != expected_path
        {
            return Ok(disconnected());
        }
        Ok::<_, zbus::Error>(linux_state::interpret(SessionSample {
            locked: Some(locked),
            active: Some(active),
            state: Some(&state),
            preparing_sleep: Some(sleeping),
        }))
    };
    tokio::time::timeout(QUERY_TIMEOUT, result)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_else(|| Observation::unknown(Reason::ObservationUnavailable))
}

struct Effect {
    reason: Option<Reason>,
    reconnect: bool,
    resample: bool,
}
impl Effect {
    fn ignored() -> Self {
        Self {
            reason: None,
            reconnect: false,
            resample: false,
        }
    }
    fn changed(reason: Reason) -> Self {
        Self {
            reason: Some(reason),
            reconnect: false,
            resample: true,
        }
    }
    fn disconnected() -> Self {
        Self {
            reason: Some(Reason::MonitorDisconnected),
            reconnect: true,
            resample: false,
        }
    }
}
fn property_reason(message: &Message) -> Option<Reason> {
    if message.data().len() > MAX_MESSAGE {
        return Some(Reason::ObservationUnavailable);
    }
    let Ok((interface, changed, invalidated)) =
        message
            .body()
            .deserialize::<(String, HashMap<String, OwnedVariant>, Vec<String>)>()
    else {
        return Some(Reason::ObservationUnavailable);
    };
    if interface != SESSION {
        return None;
    }
    if changed.len() > 32 || invalidated.len() > 32 {
        return Some(Reason::ObservationUnavailable);
    }
    let mut reasons = Vec::with_capacity(4);
    for (key, value) in &changed {
        let event = match key.as_str() {
            "LockedHint" => Event::Locked(bool::try_from(value).ok()),
            "Active" => Event::Active(bool::try_from(value).ok()),
            "State" => Event::State(<&str>::try_from(value).ok()),
            _ => continue,
        };
        reasons.push(linux_state::event_reason(event));
    }
    if invalidated
        .iter()
        .any(|key| matches!(key.as_str(), "LockedHint" | "Active" | "State"))
    {
        reasons.push(linux_state::event_reason(Event::Invalidated));
    }
    reasons.into_iter().max_by_key(|reason| match reason {
        Reason::Locked | Reason::InactiveSession => 3,
        Reason::ObservationUnavailable => 2,
        _ => 1,
    })
}
fn event_effect(message: &Message, expected_owner: &str, expected_path: &str) -> Effect {
    let header = message.header();
    let sender = header.sender().map(|value| value.as_str());
    let path = header.path().map(|value| value.as_str());
    let interface = header.interface().map(|value| value.as_str());
    let member = header.member().map(|value| value.as_str());
    if sender == Some(BUS)
        && path == Some(BUS_PATH)
        && interface == Some(BUS)
        && member == Some("NameOwnerChanged")
    {
        if message.data().len() > MAX_MESSAGE {
            return Effect::disconnected();
        }
        return match message.body().deserialize::<(String, String, String)>() {
            Ok((name, _, _)) if name == SERVICE => Effect::disconnected(),
            Ok(_) => Effect::ignored(),
            _ => Effect::disconnected(),
        };
    }
    if sender != Some(expected_owner) {
        return Effect::ignored();
    }
    if path == Some(expected_path)
        && interface == Some(PROPERTIES)
        && member == Some("PropertiesChanged")
    {
        return property_reason(message)
            .map(Effect::changed)
            .unwrap_or_else(Effect::ignored);
    }
    if path == Some(MANAGER_PATH) && interface == Some(MANAGER) {
        if message.data().len() > MAX_MESSAGE {
            return Effect::changed(Reason::ObservationUnavailable);
        }
        if member == Some("PrepareForSleep") {
            return Effect::changed(linux_state::event_reason(Event::Sleep(
                message.body().deserialize::<bool>().ok(),
            )));
        }
        if member == Some("SessionRemoved") {
            return match message.body().deserialize::<(String, OwnedObjectPath)>() {
                Ok((_, path)) if path.as_str() == expected_path => Effect::disconnected(),
                Ok(_) => Effect::ignored(),
                _ => Effect::disconnected(),
            };
        }
    }
    Effect::ignored()
}
fn observe(
    control: &Control,
    event: Option<zbus::Result<Message>>,
    owner: &str,
    path: &str,
) -> Effect {
    let effect = event
        .map(|event| {
            event
                .map(|message| event_effect(&message, owner, path))
                .unwrap_or_else(|_| Effect::disconnected())
        })
        .unwrap_or_else(Effect::disconnected);
    if let Some(reason) = effect.reason {
        control.gate.block(reason);
    }
    effect
}

// Poll observers before the query at every yield. A lock/sleep/disconnect seen
// while a read is in flight immediately fences its potentially clear result.
async fn query(control: &Control, session: &mut Session) -> (Observation, bool, bool) {
    let fence = control.gate.sample_fence();
    let connection = session.connection.clone();
    let owner = session.owner.clone();
    let path = session.path.clone();
    let operation = read_sample(&connection, owner.as_str(), path.as_str());
    tokio::pin!(operation);
    let deadline = tokio::time::sleep(QUERY_TIMEOUT);
    tokio::pin!(deadline);
    let mut resample = false;
    loop {
        tokio::select! { biased;
            _=control.stop.notified()=>return(disconnected(),true,false),
            _=connection.closed()=>{control.gate.block(Reason::MonitorDisconnected);return(disconnected(),true,false);},
            _=&mut deadline=>{let observation=Observation::unknown(Reason::ObservationUnavailable);control.gate.apply_sample(fence,observation);return(observation,false,resample);},
            event=session.events.next()=>{
                let effect=observe(control,event,owner.as_str(),path.as_str());
                if effect.reconnect { return(disconnected(),true,false); }
                resample|=effect.resample;
            },
            observation=&mut operation=>{
                if control.stopped.load(Ordering::SeqCst) { return(disconnected(),true,false); }
                control.gate.apply_sample(fence,observation);
                return(observation,observation.reason==Reason::MonitorDisconnected,resample);
            },
        }
    }
}
async fn serve(control: &Control, requests: &mut mpsc::Receiver<Request>, session: &mut Session) {
    let mut resample = true;
    loop {
        if control.stopped.load(Ordering::SeqCst) {
            return;
        }
        if resample {
            let (_, reconnect, again) = query(control, session).await;
            if reconnect {
                return;
            }
            resample = again;
        }
        tokio::select! { biased;
            _=control.stop.notified()=>return,
            _=session.connection.closed()=>return,
            event=session.events.next()=>{
                let effect=observe(control,event,session.owner.as_str(),session.path.as_str());
                if effect.reconnect { return; }
                resample|=effect.resample;
            },
            request=requests.recv()=>{
                let Some(reply)=request else { return; };
                if reply.is_closed() { continue; }
                let (observation,reconnect,again)=query(control,session).await;
                let _=reply.send(observation);
                if reconnect { return; }
                resample|=again;
            },
            // Debounce repeated invalidations without a hot resample loop.
            _=tokio::time::sleep(Duration::from_millis(100)),if resample=>{},
        }
    }
}
