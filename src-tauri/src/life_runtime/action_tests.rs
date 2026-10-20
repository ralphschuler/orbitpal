//! Real native owner + SQLite scenarios. No providers, OS APIs or renderers.
use crate::{
    action_executor::{Decision, Status},
    life_domain::ActorId,
    life_runtime::{
        action_host::{admit_rest, poll, status, visual_request},
        core::Core,
    },
    storage::Store,
    world_hub::{self, ActionContext, WorldHub},
};
use crate::{
    life_availability::{Availability, Reason, Snapshot, Source},
    life_domain::{ActionPhase, Needs, RandomState, WorldId},
    life_runtime::{
        action_state::UiProof,
        core::{Mode, SpeechConfig},
    },
    storage::world_store::OutputBinding,
    world_state::{coordinates::Rect, StageId, WorldConfig},
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{path::PathBuf, sync::atomic::AtomicU64};
struct Fixture {
    root: PathBuf,
    store: Store,
    world: WorldHub,
    core: Core,
    now: AtomicU64,
    ticket: u64,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    fn new(ids: &[&str]) -> Self {
        let root = std::env::temp_dir().join(format!("orbitpal-action-{}", uuid::Uuid::new_v4()));
        let store = Store::new(root.clone()).unwrap();
        let original = store.bot_profile("nova").unwrap();
        let db = rusqlite::Connection::open(root.join("cohost.sqlite3")).unwrap();
        for id in ids {
            let mut p = original.clone();
            p["id"] = json!(id);
            if *id != "nova" {
                p["output"]["mode"] = json!("overlay");
            }
            db.execute("INSERT INTO bot_profiles(id,profile) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET profile=excluded.profile",rusqlite::params![id,p.to_string()]).unwrap();
            store
                .initialize_life(
                    id,
                    WorldId::new(format!("lineage-{id}")).unwrap(),
                    Needs::new(4000, 4000, 4000, 4000).unwrap(),
                    RandomState::seeded(7),
                    100,
                )
                .unwrap();
        }
        let mut core = Core::new(Snapshot {
            state: Availability::AwakeUnlocked,
            reason: Reason::Ready,
            generation: 1,
            source: Source::Unsupported,
        });
        for id in ids {
            core.start(
                id,
                Mode::Life,
                uuid::Uuid::new_v4().to_string(),
                SpeechConfig {
                    speak: false,
                    min_ms: 15000,
                    max_ms: 15000,
                },
                Some(store.load_life(id).unwrap().unwrap().state().clone()),
                7,
                false,
                true,
                0,
            )
            .unwrap();
        }
        core.connect_ui("studio".into(), 0);
        assert!(core.report_ui("studio", 1, false, false, true, 0));
        let world = WorldHub::new(&store);
        let config = WorldConfig::new(
            StageId::new("stage").unwrap(),
            Rect::full(),
            ids.iter().map(|id| ActorId::new(*id).unwrap()).collect(),
            vec![],
        )
        .unwrap();
        world
            .save(
                &store,
                None,
                &config,
                &OutputBinding::default(),
                &world_hub::current_actors(&core),
                crate::world_hub::action_fixture_topology(1),
                0,
            )
            .unwrap();
        Self {
            root,
            store,
            world,
            core,
            now: AtomicU64::new(0),
            ticket: 1,
        }
    }
    fn context(&mut self, id: &str) -> ActionContext {
        self.ticket += 1;
        let at = self.now.load(Ordering::SeqCst);
        self.world
            .synchronize(
                &self.store,
                &world_hub::current_actors(&self.core),
                Some(crate::world_hub::action_fixture_topology(self.ticket)),
                at,
            )
            .unwrap();
        self.world
            .action_context(&self.store, &world_hub::current_actors(&self.core), id, at)
            .unwrap()
    }
    fn start(&mut self, id: &str) {
        let profile = self.store.bot_profile(id).unwrap();
        let context = self.context(id);
        let fence = self
            .core
            .local_fence(id, self.now.load(Ordering::SeqCst))
            .unwrap();
        admit_rest(
            &mut self.core,
            &self.store,
            &self.world,
            id,
            &profile,
            fence,
            context,
            Arc::new(AtomicBool::new(false)),
            || self.now.load(Ordering::SeqCst),
            || 100 + self.now.load(Ordering::SeqCst),
        )
        .unwrap();
    }
    fn tick(&mut self, at: u64) {
        self.now.store(at, Ordering::SeqCst);
        assert!(self
            .core
            .report_ui("studio", at + 2, false, false, true, at));
        self.ticket += 1;
        self.world
            .synchronize(
                &self.store,
                &world_hub::current_actors(&self.core),
                Some(crate::world_hub::action_fixture_topology(self.ticket)),
                at,
            )
            .unwrap();
        poll(
            &mut self.core,
            &self.store,
            &self.world,
            || self.now.load(Ordering::SeqCst),
            || 100 + self.now.load(Ordering::SeqCst),
        )
        .unwrap();
    }
}
pub(crate) fn run(case: &str) {
    match case {
        "complete" => {
            let mut f = Fixture::new(&["nova"]);
            let needs = f.store.load_life("nova").unwrap().unwrap().state().needs();
            f.start("nova");
            assert_eq!(
                status(&f.core, &f.store, &f.world, "nova", 0).unwrap()["runtime"]["status"],
                "started"
            );
            for t in 1..=10 {
                f.tick(t * 1000);
            }
            let state = f.store.load_life("nova").unwrap().unwrap();
            assert_eq!(
                state.state().current_action().unwrap().phase(),
                ActionPhase::Completed
            );
            assert_eq!(state.state().needs(), needs);
            let before = serde_json::to_value(state.state()).unwrap();
            f.tick(11000);
            assert_eq!(
                before,
                serde_json::to_value(f.store.load_life("nova").unwrap().unwrap().state()).unwrap()
            );
            assert!(visual_request().is_err());
            assert_eq!(
                before,
                serde_json::to_value(f.store.load_life("nova").unwrap().unwrap().state()).unwrap()
            );
        }
        "stop" => {
            let mut f = Fixture::new(&["nova", "other"]);
            f.start("nova");
            f.start("other");
            let other = f.core.actions.as_ref().unwrap().jobs["other"].clone();
            f.core.stop("nova", 1);
            f.world.revoke_actor(&f.store, "nova");
            poll(&mut f.core, &f.store, &f.world, || 1, || 101).unwrap();
            assert_eq!(
                f.store
                    .load_life("nova")
                    .unwrap()
                    .unwrap()
                    .state()
                    .current_action()
                    .unwrap()
                    .phase(),
                ActionPhase::Cancelled
            );
            assert_eq!(
                f.core.actions.as_ref().unwrap().executor.status(&other.key),
                Some(Status::Started)
            );
            assert!(!other.cancel.load(Ordering::SeqCst));
            for t in 1..=10 {
                f.tick(t * 1000);
            }
            assert_eq!(
                f.store
                    .load_life("other")
                    .unwrap()
                    .unwrap()
                    .state()
                    .current_action()
                    .unwrap()
                    .phase(),
                ActionPhase::Completed
            );
        }
        "intent" => {
            let mut f = Fixture::new(&["nova"]);
            let profile = f.store.bot_profile("nova").unwrap();
            let proof = UiProof {
                connection_id: "studio".into(),
                sequence: 1,
            };
            f.core
                .prepare_rest_intent("nova", profile.clone(), &proof, 0, 0);
            let intent = f.core.rest_intent("nova", &proof).unwrap();
            let fence = f.core.consume_rest_intent(&intent, &profile, 1).unwrap();
            assert!(f.core.local_current("nova", &fence, 1));
            assert!(f.core.consume_rest_intent(&intent, &profile, 1).is_none());
            assert!(f.core.report_ui("studio", 2, false, false, true, 2));
            let proof = UiProof {
                connection_id: "studio".into(),
                sequence: 2,
            };
            f.core
                .prepare_rest_intent("nova", profile.clone(), &proof, 0, 2);
            let old = f.core.rest_intent("nova", &proof).unwrap();
            assert!(f.core.report_ui("studio", 3, true, true, true, 3));
            assert!(f.core.consume_rest_intent(&old, &profile, 3).is_none());
            assert!(!f.core.report_ui("studio", 2, false, false, true, 4));
            assert!(f.core.consume_rest_intent(&old, &profile, 4).is_none());
        }
        "invalidate" => {
            for reason in ["profile", "world", "output", "system", "ui"] {
                let mut f = Fixture::new(&["nova"]);
                f.start("nova");
                match reason {
                    "profile" => {
                        let db = rusqlite::Connection::open(f.root.join("cohost.sqlite3")).unwrap();
                        let mut p = f.store.bot_profile("nova").unwrap();
                        p["avatar"]["scale"] = json!(2);
                        db.execute(
                            "UPDATE bot_profiles SET profile=?1 WHERE id='nova'",
                            [p.to_string()],
                        )
                        .unwrap();
                    }
                    "world" => {
                        let record = f.store.load_world_config().unwrap().unwrap();
                        f.store
                            .save_world_config(Some(1), &record.config, &record.output_binding)
                            .unwrap();
                    }
                    "output" => {
                        let s = &f.core.sessions["nova"];
                        let (id, rev) = (s.session_id.clone(), s.revision);
                        f.core.set_output_available("nova", &id, rev, false, 1);
                    }
                    "system" => f.core.observe_system(
                        Snapshot {
                            state: Availability::SuspendedLocked,
                            reason: Reason::Locked,
                            generation: 2,
                            source: Source::Unsupported,
                        },
                        1,
                    ),
                    _ => {
                        f.core.report_ui("studio", 2, true, true, true, 1);
                    }
                }
                poll(&mut f.core, &f.store, &f.world, || 1, || 101).unwrap();
                assert_eq!(
                    f.store
                        .load_life("nova")
                        .unwrap()
                        .unwrap()
                        .state()
                        .current_action()
                        .unwrap()
                        .phase(),
                    ActionPhase::Cancelled,
                    "{reason}"
                );
            }
        }
        "wait_expiry" | "wait_profile" | "wait_world" | "wait_selected" | "wait_output" => {
            let mut f = Fixture::new(&["nova"]);
            let profile = f.store.bot_profile("nova").unwrap();
            let ctx = f.context("nova");
            let fence = f.core.local_fence("nova", 0).unwrap();
            let path = f.root.join("cohost.sqlite3");
            let output = crate::life_runtime::action_output::OutputGate::default();
            output.ui_connect("studio");
            output.ui_report("studio", 1, false);
            let cancel = Arc::new(AtomicBool::new(false));
            assert!(output.watch(
                output.ticket().unwrap(),
                "nova",
                "main",
                &cancel,
                ("studio", 1)
            ));
            let gate = std::sync::Barrier::new(2);
            let at = &f.now;
            let core = &mut f.core;
            let store = &f.store;
            let world = &f.world;
            std::thread::scope(|scope| {
                let writer=scope.spawn(||{let db=rusqlite::Connection::open(path).unwrap();db.execute_batch("BEGIN IMMEDIATE").unwrap();gate.wait();std::thread::sleep(std::time::Duration::from_millis(80));match case {"wait_expiry"=>{at.store(6000,Ordering::SeqCst);},"wait_profile"=>{let mut p=profile.clone();p["avatar"]["scale"]=json!(2);db.execute("UPDATE bot_profiles SET profile=?1 WHERE id='nova'",[p.to_string()]).unwrap();},"wait_selected"=>{db.execute("UPDATE settings SET value=json_set(value,'$.id','other') WHERE key='profile'",[]).unwrap();},"wait_output"=>{output.window("main");},_=>{db.execute("UPDATE shared_world_config SET revision=revision+1,config_json=json_set(config_json,'$.revision',2)",[]).unwrap();}}db.execute_batch("COMMIT").unwrap();});
                gate.wait();
                assert!(admit_rest(
                    core,
                    store,
                    world,
                    "nova",
                    &profile,
                    fence,
                    ctx,
                    cancel.clone(),
                    || at.load(Ordering::SeqCst),
                    || 100
                )
                .is_err());
                writer.join().unwrap();
            });
            assert!(f
                .store
                .load_life("nova")
                .unwrap()
                .unwrap()
                .state()
                .current_action()
                .is_none());
        }
        "install_failure" | "install_cancel_failure" => {
            let mut f = Fixture::new(&["nova"]);
            let profile = f.store.bot_profile("nova").unwrap();
            let ctx = f.context("nova");
            let fence = f.core.local_fence("nova", 0).unwrap();
            let calls = std::cell::Cell::new(0);
            let path = f.root.join("cohost.sqlite3");
            let now = || {
                let n = calls.get() + 1;
                calls.set(n);
                if n == 3 {
                    if case == "install_cancel_failure" {
                        let db = rusqlite::Connection::open(&path).unwrap();
                        db.execute_batch("CREATE TRIGGER fail_action_cancel BEFORE UPDATE ON bot_life_states BEGIN SELECT RAISE(ABORT,'synthetic cancel failure');END;").unwrap();
                    }
                    crate::life_domain::MAX_COUNTER
                } else {
                    0
                }
            };
            assert!(admit_rest(
                &mut f.core,
                &f.store,
                &f.world,
                "nova",
                &profile,
                fence,
                ctx,
                Arc::new(AtomicBool::new(false)),
                now,
                || 100
            )
            .is_err());
            let actual = f.store.load_life("nova").unwrap().unwrap();
            assert_eq!(
                actual.state().current_action().unwrap().phase(),
                if case == "install_cancel_failure" {
                    ActionPhase::Planned
                } else {
                    ActionPhase::Cancelled
                }
            );
            assert_eq!(
                f.core.sessions["nova"].state.as_ref().unwrap(),
                actual.state()
            );
            assert_eq!(
                f.core.sessions["nova"].storage_failed,
                case == "install_cancel_failure"
            );
            assert!(f.core.actions.as_ref().unwrap().jobs.is_empty());
            assert!(
                f.core.local_fence("nova", 0).is_none()
                    || !f
                        .core
                        .actions
                        .as_ref()
                        .unwrap()
                        .executor
                        .can_admit(&ActorId::new("nova").unwrap())
            );
        }
        "retire_capacity" | "terminal_capacity" => {
            let mut f = Fixture::new(&["nova"]);
            let original = f.store.bot_profile("nova").unwrap();
            let mut first = None;
            for n in 0..64 {
                let id = format!("broken-{n}");
                let mut profile = original.clone();
                profile["id"] = json!(id);
                profile["output"]["mode"] = json!("overlay");
                let db = rusqlite::Connection::open(f.root.join("cohost.sqlite3")).unwrap();
                db.execute(
                    "INSERT INTO bot_profiles(id,profile) VALUES(?1,?2)",
                    rusqlite::params![id, profile.to_string()],
                )
                .unwrap();
                f.store
                    .initialize_life(
                        &id,
                        WorldId::new(format!("lineage-{id}")).unwrap(),
                        Needs::new(4000, 4000, 4000, 4000).unwrap(),
                        RandomState::seeded(7),
                        100,
                    )
                    .unwrap();
                f.core.stop("nova", 0);
                f.core
                    .start(
                        &id,
                        Mode::Life,
                        uuid::Uuid::new_v4().to_string(),
                        SpeechConfig {
                            speak: false,
                            min_ms: 15000,
                            max_ms: 15000,
                        },
                        Some(f.store.load_life(&id).unwrap().unwrap().state().clone()),
                        7,
                        false,
                        true,
                        0,
                    )
                    .unwrap();
                let record = f.store.load_world_config().unwrap().unwrap();
                let config = WorldConfig::new(
                    record.config.stage_id().clone(),
                    Rect::full(),
                    vec![ActorId::new(&id).unwrap()],
                    vec![],
                )
                .unwrap()
                .with_revision(record.config.revision())
                .unwrap();
                f.ticket += 1;
                f.world
                    .save(
                        &f.store,
                        Some(record.config.revision()),
                        &config,
                        &record.output_binding,
                        &world_hub::current_actors(&f.core),
                        crate::world_hub::action_fixture_topology(f.ticket),
                        0,
                    )
                    .unwrap();
                f.start(&id);
                let job = f.core.actions.as_ref().unwrap().jobs[&id].clone();
                if first.is_none() {
                    first = Some(job.clone());
                }
                if case == "terminal_capacity" {
                    let base = f.now.load(Ordering::SeqCst);
                    for n in 1..=10 {
                        f.tick(base + n * 1000);
                    }
                    assert!(!f.core.actions.as_ref().unwrap().jobs.contains_key(&id));
                    assert_eq!(
                        f.store
                            .load_life(&id)
                            .unwrap()
                            .unwrap()
                            .state()
                            .current_action()
                            .unwrap()
                            .phase(),
                        ActionPhase::Completed
                    );
                    f.core.stop(&id, f.now.load(Ordering::SeqCst));
                    // Keep monotonic source times while rotating explicit sessions.
                    continue;
                }
                db.execute_batch("CREATE TRIGGER fail_action_cancel BEFORE UPDATE ON bot_life_states BEGIN SELECT RAISE(ABORT,'synthetic cancel failure');END;").unwrap();
                let before: String = db
                    .query_row(
                        "SELECT state_json FROM bot_life_states WHERE bot_id=?1",
                        [&id],
                        |r| r.get(0),
                    )
                    .unwrap();
                f.core.stop(&id, 0);
                f.world.revoke_actor(&f.store, &id);
                poll(&mut f.core, &f.store, &f.world, || 0, || 100).unwrap();
                assert!(!f.core.sessions.contains_key(&id));
                assert!(!f.core.actions.as_ref().unwrap().jobs.contains_key(&id));
                assert_eq!(
                    before,
                    db.query_row(
                        "SELECT state_json FROM bot_life_states WHERE bot_id=?1",
                        [&id],
                        |r| r.get::<_, String>(0)
                    )
                    .unwrap()
                );
                assert_eq!(
                    f.core
                        .actions
                        .as_ref()
                        .unwrap()
                        .executor
                        .snapshot(&job.key)
                        .is_none(),
                    true
                );
                db.execute_batch("DROP TRIGGER fail_action_cancel").unwrap();
            }
            assert_eq!(f.core.actions.as_ref().unwrap().retired.len(), 64);
            let old = first.unwrap();
            let actual = f.store.load_life("broken-0").unwrap().unwrap();
            assert!(f
                .core
                .actions
                .as_mut()
                .unwrap()
                .executor
                .start_native_dwell(&old.authority, &old.key, actual.state(), 0)
                .is_err());
            f.core
                .start(
                    "nova",
                    Mode::Life,
                    uuid::Uuid::new_v4().to_string(),
                    SpeechConfig {
                        speak: false,
                        min_ms: 15000,
                        max_ms: 15000,
                    },
                    Some(f.store.load_life("nova").unwrap().unwrap().state().clone()),
                    7,
                    false,
                    true,
                    0,
                )
                .unwrap();
            let record = f.store.load_world_config().unwrap().unwrap();
            let config = WorldConfig::new(
                record.config.stage_id().clone(),
                Rect::full(),
                vec![ActorId::new("nova").unwrap()],
                vec![],
            )
            .unwrap()
            .with_revision(record.config.revision())
            .unwrap();
            f.ticket += 1;
            f.world
                .save(
                    &f.store,
                    Some(record.config.revision()),
                    &config,
                    &record.output_binding,
                    &world_hub::current_actors(&f.core),
                    crate::world_hub::action_fixture_topology(f.ticket),
                    0,
                )
                .unwrap();
            f.start("nova");
            let base = f.now.load(Ordering::SeqCst);
            for n in 1..=10 {
                f.tick(base + n * 1000);
            }
            assert_eq!(
                f.store
                    .load_life("nova")
                    .unwrap()
                    .unwrap()
                    .state()
                    .current_action()
                    .unwrap()
                    .phase(),
                ActionPhase::Completed
            );
        }
        "retire_permit" => {
            let mut f = Fixture::new(&["nova"]);
            f.start("nova");
            let job = f.core.actions.as_ref().unwrap().jobs["nova"].clone();
            let state = f.store.load_life("nova").unwrap().unwrap();
            let executor = &mut f.core.actions.as_mut().unwrap().executor;
            for t in [2000, 4000, 6000, 8000] {
                assert!(matches!(
                    executor
                        .tick_native_dwell(&job.authority, &job.key, state.state(), t)
                        .unwrap(),
                    Decision::Accepted
                ));
            }
            let Decision::Commit(request) = executor
                .tick_native_dwell(&job.authority, &job.key, state.state(), 10000)
                .unwrap()
            else {
                panic!("missing completion proposal")
            };
            let permit = executor
                .authorize_commit(&job.authority, &request, state.state(), 10000)
                .unwrap();
            let db = rusqlite::Connection::open(f.root.join("cohost.sqlite3")).unwrap();
            db.execute_batch("CREATE TRIGGER fail_action_cancel BEFORE UPDATE ON bot_life_states BEGIN SELECT RAISE(ABORT,'synthetic cancel failure');END;").unwrap();
            f.core.stop("nova", 10000);
            f.world.revoke_actor(&f.store, "nova");
            poll(&mut f.core, &f.store, &f.world, || 10000, || 10100).unwrap();
            assert!(f.core.actions.as_ref().unwrap().jobs.is_empty());
            assert!(f
                .core
                .actions
                .as_mut()
                .unwrap()
                .executor
                .confirm_commit(&job.authority, permit, state.state(), 10000)
                .is_err());
            assert_eq!(
                f.store.load_life("nova").unwrap().unwrap().state(),
                state.state()
            );
        }
        "complete_wait_output" => {
            let mut f = Fixture::new(&["nova"]);
            f.start("nova");
            for t in 1..10 {
                f.tick(t * 1000);
            }
            let job = f.core.actions.as_ref().unwrap().jobs["nova"].clone();
            let output = crate::life_runtime::action_output::OutputGate::default();
            output.ui_connect("studio");
            output.ui_report("studio", 1, false);
            assert!(output.watch(
                output.ticket().unwrap(),
                "nova",
                "main",
                &job.cancel,
                ("studio", 1)
            ));
            f.now.store(10000, Ordering::SeqCst);
            assert!(f.core.report_ui("studio", 10002, false, false, true, 10000));
            f.ticket += 1;
            f.world
                .synchronize(
                    &f.store,
                    &world_hub::current_actors(&f.core),
                    Some(crate::world_hub::action_fixture_topology(f.ticket)),
                    10000,
                )
                .unwrap();
            let path = f.root.join("cohost.sqlite3");
            let barrier = std::sync::Barrier::new(2);
            std::thread::scope(|scope| {
                let writer = scope.spawn(|| {
                    let db = rusqlite::Connection::open(path).unwrap();
                    db.execute_batch("BEGIN IMMEDIATE").unwrap();
                    barrier.wait();
                    std::thread::sleep(std::time::Duration::from_millis(80));
                    output.window("main");
                    db.execute_batch("COMMIT").unwrap();
                });
                barrier.wait();
                poll(&mut f.core, &f.store, &f.world, || 10000, || 10100).unwrap();
                writer.join().unwrap();
            });
            assert_eq!(
                f.store
                    .load_life("nova")
                    .unwrap()
                    .unwrap()
                    .state()
                    .current_action()
                    .unwrap()
                    .phase(),
                ActionPhase::Cancelled
            );
        }
        "output_registry" => {
            let gate = crate::life_runtime::action_output::OutputGate::default();
            gate.ui_connect("studio");
            gate.ui_report("studio", 1, false);
            let ticket = gate.ticket().unwrap();
            let a = Arc::new(AtomicBool::new(false));
            let b = Arc::new(AtomicBool::new(false));
            assert!(gate.watch(ticket, "a", "avatar-a", &a, ("studio", 1)));
            assert!(gate.watch(ticket, "b", "avatar-b", &b, ("studio", 1)));
            gate.window("avatar-a");
            assert!(a.load(Ordering::SeqCst));
            assert!(!b.load(Ordering::SeqCst));
            let stale = Arc::new(AtomicBool::new(false));
            assert!(!gate.watch(ticket, "new", "avatar-new", &stale, ("studio", 1)));
            assert!(stale.load(Ordering::SeqCst));
            gate.bot(Some("b"));
            assert!(b.load(Ordering::SeqCst));
            let ticket = gate.ticket().unwrap();
            let mut flags = Vec::new();
            for i in 0..64 {
                let c = Arc::new(AtomicBool::new(false));
                assert!(gate.watch(ticket, &format!("bot-{i}"), "main", &c, ("studio", 1)));
                for _ in 0..3 {
                    assert!(gate.watch(ticket, &format!("bot-{i}"), "main", &c, ("studio", 1)));
                }
                flags.push(c);
            }
            let extra = Arc::new(AtomicBool::new(false));
            assert!(!gate.watch(ticket, "overflow", "main", &extra, ("studio", 1)));
            gate.window("main");
            assert!(flags.iter().all(|c| c.load(Ordering::SeqCst)));
            assert!(gate.watch(
                gate.ticket().unwrap(),
                "recovered",
                "main",
                &Arc::new(AtomicBool::new(false)),
                ("studio", 1)
            ));
        }
        "output_ui_identity" => {
            let gate = crate::life_runtime::action_output::OutputGate::default();
            gate.ui_connect("old");
            gate.ui_report("old", 1, false);
            let a = Arc::new(AtomicBool::new(false));
            assert!(gate.watch(gate.ticket().unwrap(), "nova", "main", &a, ("old", 1)));
            gate.ui_connect("new");
            assert!(a.load(Ordering::SeqCst));
            gate.ui_report("new", 1, false);
            let b = Arc::new(AtomicBool::new(false));
            assert!(gate.watch(gate.ticket().unwrap(), "nova", "main", &b, ("new", 1)));
            gate.ui_disconnect("old");
            gate.ui_report("old", 999, true);
            gate.ui_report("new", 1, true);
            assert!(!b.load(Ordering::SeqCst));
            gate.ui_report("new", 2, true);
            assert!(b.load(Ordering::SeqCst));
        }
        "output_intent_interleaving" => {
            let mut f = Fixture::new(&["nova"]);
            let gate = crate::life_runtime::action_output::OutputGate::default();
            gate.ui_connect("studio");
            gate.ui_report("studio", 1, true);
            gate.bot(None);
            let intent_generation = gate.ticket().unwrap();
            let proof = UiProof {
                connection_id: "studio".into(),
                sequence: 1,
            };
            let profile = f.store.bot_profile("nova").unwrap();
            f.core
                .prepare_rest_intent("nova", profile.clone(), &proof, intent_generation, 0);
            let intent = f.core.rest_intent("nova", &proof).unwrap();
            // Core A intentionally remains locked/unchanged while native connect B is published.
            gate.ui_connect("new");
            gate.ui_report("new", 1, false);
            let cancel = Arc::new(AtomicBool::new(false));
            assert!(!gate.watch(
                gate.ticket().unwrap(),
                "nova",
                "main",
                &cancel,
                (&intent.connection, intent.report_sequence)
            ));
            assert!(cancel.load(Ordering::SeqCst));
            assert!(f
                .store
                .load_life("nova")
                .unwrap()
                .unwrap()
                .state()
                .current_action()
                .is_none());
            gate.ui_connect("studio");
            gate.ui_report("studio", 1, true);
            let ticket = gate.ticket().unwrap();
            gate.ui_report("studio", 2, false);
            let clear = Arc::new(AtomicBool::new(false));
            assert!(gate.watch(ticket, "nova", "main", &clear, ("studio", 1)));
            gate.ui_report("studio", 3, true);
            let late = Arc::new(AtomicBool::new(false));
            assert!(!gate.watch(gate.ticket().unwrap(), "nova", "main", &late, ("studio", 1)));
            assert!(late.load(Ordering::SeqCst));
        }
        _ => panic!("unknown action scenario"),
    }
}
#[test]
fn native_rest_real_store_zero_effect_exactly_once() {
    run("complete")
}
#[test]
fn native_core_and_host_targeted_stop_preserves_other_rest() {
    run("stop")
}
#[test]
fn explicit_intent_is_single_use_and_real_block_revokes() {
    run("intent")
}
#[test]
fn native_rest_current_fences_cancel_without_completion() {
    run("invalidate")
}
#[test]
fn after_db_wait_expired_ui_cannot_plan() {
    run("wait_expiry")
}
#[test]
fn after_db_wait_changed_profile_cannot_plan() {
    run("wait_profile")
}
#[test]
fn after_db_wait_changed_world_cannot_plan() {
    run("wait_world")
}

#[test]
fn installed_plan_failure_has_real_confirmed_cancel() {
    run("install_failure")
}
#[test]
fn failed_plan_cancel_keeps_actual_plan_and_storage_unavailable() {
    run("install_cancel_failure")
}
#[test]
fn sixty_four_failed_stores_retire_without_resetting_life_or_blocking_healthy_actor() {
    run("retire_capacity")
}

#[test]
fn retired_failed_runtime_rejects_old_unused_completion_permit() {
    run("retire_permit")
}

#[test]
fn selected_studio_actor_changed_during_db_wait_cannot_plan() {
    run("wait_selected")
}
#[test]
fn native_output_revoked_during_db_wait_cannot_plan() {
    run("wait_output")
}
#[test]
fn completed_actor_rotation_reclaims_bounded_runtime_slots() {
    run("terminal_capacity")
}

#[test]
fn completion_output_revoked_during_db_wait_cannot_commit_success() {
    run("complete_wait_output")
}
#[test]
fn output_registry_is_bounded_identity_scoped_and_stale_registration_fails() {
    run("output_registry")
}

#[test]
fn old_ui_identity_or_sequence_cannot_revoke_new_output_job() {
    run("output_ui_identity")
}

#[test]
fn pending_core_intent_cannot_register_after_new_ui_or_later_block() {
    run("output_intent_interleaving")
}
