//! Synchronous actual Core -> WorldHub -> Store action owner. No GUI/provider I/O.
use super::{
    action_state::{LocalFence, NativeActions, NativeJob},
    core::Core,
};
use crate::{
    action_executor::{
        self as ex, Capabilities, CommitRequest, Decision, Failure, Goal, NativeAuthority,
        OutputStamp, PreparedPlan, Presentation, Status,
    },
    life_domain::{Activity, ActorId, CancelReason, LifeEvent},
    storage::{
        life_store::{LifeSnapshot, LifeStoreError, WriteOutcome},
        Store,
    },
    world_hub::{self, ActionContext, WorldHub},
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
pub(super) const UNAVAILABLE: &str =
    "Die stille Pause ist momentan nicht verfügbar. Prüfe Life, Studio, Bühne und Ausgabe.";
pub(super) const UNSUPPORTED:&str="Visuelle Lebensaktionen benötigen eine bestätigte Darstellung. Diese Verbindung folgt in einer späteren Version.";
const SLOT_LEASE_MS: u64 = 5_000;
fn invalid(_: impl std::fmt::Debug) -> String {
    UNAVAILABLE.into()
}
fn with_actions<T>(
    core: &mut Core,
    f: impl FnOnce(&mut NativeActions, &mut Core) -> Result<T, String>,
) -> Result<T, String> {
    let mut actions = core.actions.take().ok_or(UNAVAILABLE)?;
    let result = f(&mut actions, core);
    core.actions = Some(actions);
    result
}
fn current(core: &Core, job: &NativeJob, now: u64) -> bool {
    !job.cancel.load(Ordering::SeqCst)
        && core.local_current(job.key.actor().as_str(), &job.fence, now)
        && job.slot.as_ref().is_none_or(|slot| slot.fresh(now))
        && job.native_permit.as_ref().is_none_or(|permit|
            permit.current(core, now) && job.visual.as_ref().map(|visual|&visual.output).or(job.native_rest_output.as_ref()).is_some_and(|output|permit.matches_output(output)&&core.world_outputs.check_proof(output,now)))
        && job
            .visual
            .as_ref()
            .is_none_or(|visual| visual.current(&core.world_outputs, now))
}
fn replace(core: &mut Core, job: &NativeJob, snapshot: &LifeSnapshot) {
    core.replace_confirmed(
        job.key.actor().as_str(),
        &job.fence.session,
        job.fence.revision,
        snapshot.state().clone(),
        None,
    );
}
/// Activities without a concrete native rendering protocol remain unsupported.
/// No Plan/ordinal or visual Offer is fabricated on this error path.
pub(crate) fn visual_request() -> Result<(), String> {
    Err(UNSUPPORTED.into())
}

pub(crate) fn admit_rest(
    core: &mut Core,
    store: &Store,
    world: &WorldHub,
    bot: &str,
    profile: &Value,
    fence: LocalFence,
    context: ActionContext,
    cancel: Arc<AtomicBool>,
    now: impl Fn() -> u64,
    wall: impl Fn() -> u64,
) -> Result<(), String> {
    admit_rest_bound(core,store,world,bot,profile,fence,context,None,cancel,now,wall)
}

/// Zero-effect Here rest under the actual owner. This supplies no station
/// animation or renderer ACK; the existing native dwell clock remains truthful.
pub(crate) fn admit_native_rest(
    core: &mut Core,
    store: &Store,
    world: &WorldHub,
    bot: &str,
    profile: &Value,
    fence: LocalFence,
    context: ActionContext,
    output: crate::world_output::OutputProof,
    permit: super::native_scene::NativeActionPermit,
    cancel: Arc<AtomicBool>,
    now: impl Fn() -> u64,
    wall: impl Fn() -> u64,
) -> Result<(), String> {
    admit_rest_bound(core,store,world,bot,profile,fence,context,Some((output,permit)),cancel,now,wall)
}
fn admit_rest_bound(
    core: &mut Core,
    store: &Store,
    world: &WorldHub,
    bot: &str,
    profile: &Value,
    fence: LocalFence,
    context: ActionContext,
    native: Option<(crate::world_output::OutputProof,super::native_scene::NativeActionPermit)>,
    cancel: Arc<AtomicBool>,
    now: impl Fn() -> u64,
    wall: impl Fn() -> u64,
) -> Result<(), String> {
    with_actions(core, |actions, core| {
        let actor = ActorId::new(bot).map_err(invalid)?;
        actions.forget(bot);
        if actions.actor_busy(bot)
            || !actions.executor.can_admit(&actor)
            || !core.local_current(bot, &fence, now())
            || cancel.load(Ordering::SeqCst)
        {
            return Err(UNAVAILABLE.into());
        }
        if native.as_ref().is_some_and(|(output,permit)| !permit.current(core,now())||!permit.matches_output(output)||!core.world_outputs.check_proof(output,now())) {return Err(UNAVAILABLE.into());}
        let output_stamp=match &native {
            Some((output,_))=>output.stamp.clone(),
            None=>OutputStamp::new(format!("native-rest-{}",fence.session),fence.revision).map_err(invalid)?,
        };
        let authority = NativeAuthority::new(
            context.session.clone(),
            context.source_world.clone(),
            context.stage.clone(),
            context.config_revision,
            context.topology_revision,
            output_stamp,
            native.as_ref().map_or(fence.revision,|(output,_)|output.binding.asset_revision),
        )
        .map_err(invalid)?;
        let caps = Capabilities::new(authority.clone(), vec![], true).map_err(invalid)?;
        let persisted_profile = store.action_profile_token(bot, profile).map_err(invalid)?;
        if native.as_ref().is_some_and(|(output,_)|output.profile_token!=persisted_profile){return Err(UNAVAILABLE.into());}
        let (commit, prepared) = world.with_action_context(store, &context, || {
            if store.bot_profile(bot).map_err(invalid)? != *profile {
                return Err(UNAVAILABLE.into());
            }
            store
                .apply_life_action_checked(bot, wall(), |actual, tx| {
                    if cancel.load(Ordering::SeqCst)
                        || native.as_ref().is_some_and(|(output,permit)|!permit.current(core,now())||!permit.matches_output(output)||!core.world_outputs.check_proof(output,now()))
                        || !core.local_current(bot, &fence, now())
                        || !crate::storage::action_store::matches(
                            tx,
                            bot,
                            &persisted_profile,
                            &context.record,
                            native.is_none() && profile["output"]["mode"] != "overlay",
                        )
                    {
                        return Err(LifeStoreError::InvalidInput);
                    }
                    let plan = PreparedPlan::new(
                        authority.clone(),
                        actual.state(),
                        actual.random(),
                        Activity::Rest,
                        Goal::Here(context.position),
                        Presentation::NativeRestDwell,
                        &caps,
                    )
                    .map_err(|_| LifeStoreError::InvalidInput)?;
                    Ok((plan.event().clone(), plan.expected_revision(), plan))
                })
                .map_err(invalid)
        })?;
        if commit.outcome != WriteOutcome::Applied {
            return Err(UNAVAILABLE.into());
        }
        let planned = commit.snapshot;
        core.replace_confirmed(
            bot,
            &fence.session,
            fence.revision,
            planned.state().clone(),
            None,
        );
        let key = match actions
            .executor
            .install_plan(prepared, planned.state(), None, now())
        {
            Ok(key) => key,
            Err(_) => {
                // Real Plan exists. Never leave it optimistically runnable after install failure.
                let cancellation = planned
                    .state()
                    .current_action()
                    .ok_or(UNAVAILABLE)
                    .and_then(|action| {
                        store
                            .apply_life_action_checked(bot, wall(), |actual, _tx| {
                                if actual.state().current_action().is_none_or(|current| {
                                    current.intent().id() != action.intent().id()
                                }) {
                                    return Err(LifeStoreError::InvalidInput);
                                }
                                Ok((
                                    LifeEvent::Cancel {
                                        action_id: action.intent().id().clone(),
                                        reason: CancelReason::Unavailable,
                                    },
                                    actual.state().revision(),
                                    (),
                                ))
                            })
                            .map_err(|_| UNAVAILABLE)
                    });
                match cancellation {
                    Ok((commit, _)) => {
                        core.replace_confirmed(
                            bot,
                            &fence.session,
                            fence.revision,
                            commit.snapshot.state().clone(),
                            None,
                        );
                    }
                    Err(_) => {
                        core.mark_storage_failure(bot, now());
                        world.revoke_actor(store, bot);
                    }
                }
                return Err(UNAVAILABLE.into());
            }
        };
        let job = NativeJob {
            visual: None,
            native_rest_output: native.as_ref().map(|(output,_)|output.clone()),
            native_permit: native.map(|(_,permit)|permit),
            slot: None,
            authority,
            key,
            fence,
            world: context,
            profile: profile.clone(),
            persisted_profile,
            cancel,
        };
        replace(core, &job, &planned);
        actions.jobs.insert(bot.into(), job);
        let job = actions.jobs.get(bot).cloned().ok_or(UNAVAILABLE)?;
        let decision = actions
            .executor
            .start_native_dwell(&job.authority, &job.key, planned.state(), now())
            .map_err(invalid)?;
        if let Decision::Commit(request) = decision {
            if let Err(failure) = commit_request(
                actions, core, store, world, &job, request, false, &now, &wall,
            ) {
                job.cancel.store(true, Ordering::SeqCst);
                let _ = actions
                    .executor
                    .fail(&job.authority, &job.key, failure, now());
                if failure == Failure::Storage {
                    core.mark_storage_failure(bot, now());
                }
                return Err(UNAVAILABLE.into());
            }
        }
        actions.changed();
        Ok(())
    })
}
/// The token authorizes only the exact actual database snapshot/event. No await
/// occurs from authorization through SQLite commit and confirmed installation.
fn commit_request(
    actions: &mut NativeActions,
    core: &mut Core,
    store: &Store,
    world: &WorldHub,
    job: &NativeJob,
    request: CommitRequest,
    cancelling: bool,
    now: &impl Fn() -> u64,
    wall: &impl Fn() -> u64,
) -> Result<(), Failure> {
    let bot = job.key.actor().as_str();
    let mut operation = || -> Result<(), Failure> {
        if !cancelling
            && (store.bot_profile(bot).ok().as_ref() != Some(&job.profile)
                || !current(core, job, now()))
        {
            return Err(Failure::Interrupted);
        }
        let (commit, permit) = store
            .apply_life_action_checked(bot, wall(), |actual, tx| {
                if !cancelling
                    && (!current(core, job, now())
                        || job
                            .visual
                            .as_ref()
                            .is_some_and(|visual| !visual.fresh(now()))
                        || !crate::storage::action_store::matches(
                            tx,
                            bot,
                            &job.persisted_profile,
                            &job.world.record,
                            job.native_permit.is_none() && job.profile["output"]["mode"] != "overlay",
                        ))
                {
                    return Err(LifeStoreError::InvalidInput);
                }
                let permit = match actions.executor.authorize_commit(
                    &job.authority,
                    &request,
                    actual.state(),
                    now(),
                ) {
                    Ok(permit) => permit,
                    Err(_) => {
                        let revised = actions
                            .executor
                            .refresh_commit_request(&job.authority, &request, actual.state(), now())
                            .map_err(|_| LifeStoreError::InvalidInput)?;
                        actions
                            .executor
                            .authorize_commit(&job.authority, &revised, actual.state(), now())
                            .map_err(|_| LifeStoreError::InvalidInput)?
                    }
                };
                if cancelling && !matches!(permit.event(), LifeEvent::Cancel { .. }) {
                    return Err(LifeStoreError::InvalidInput);
                }
                Ok((permit.event().clone(), permit.expected_revision(), permit))
            })
            .map_err(|error| match error {
                LifeStoreError::InvalidInput
                | LifeStoreError::RevisionConflict
                | LifeStoreError::Domain(_) => Failure::Interrupted,
                _ => Failure::Storage,
            })?;
        replace(core, job, &commit.snapshot);
        actions
            .executor
            .confirm_commit(&job.authority, permit, commit.snapshot.state(), now())
            .map_err(|_| Failure::Protocol)
    };
    if cancelling {
        operation()
    } else {
        if let Some(slot) = &job.slot {
            world.with_action_slot_context(store, &job.world, slot, now, || Ok(operation()))
                .map_err(|_| Failure::Interrupted)?
        } else {
            world.with_action_context(store, &job.world, || Ok(operation()))
                .map_err(|_| Failure::Interrupted)?
        }
    }
}

pub(crate) fn poll(
    core: &mut Core,
    store: &Store,
    world: &WorldHub,
    now: impl Fn() -> u64,
    wall: impl Fn() -> u64,
) -> Result<(), String> {
    with_actions(core, |actions, core| {
        let _ = actions.executor.advance(now());
        let ids = actions.jobs.keys().cloned().collect::<Vec<_>>();
        for id in ids {
            let Some(mut job) = actions.jobs.get(&id).cloned() else {
                continue;
            };
            let status = actions.executor.status(&job.key);
            if matches!(
                status,
                Some(
                    Status::AwaitingDelivery | Status::Delivered | Status::Started | Status::Paused
                )
            ) {
                let valid = current(core, &job, now())
                    && store.bot_profile(&id).ok().as_ref() == Some(&job.profile)
                    && if let Some(slot) = &mut job.slot {
                        world.renew_action_slot_current(core, store, &job.world, slot, SLOT_LEASE_MS, &now).is_ok()
                    } else {
                        world.with_action_context(store, &job.world, || Ok(())).is_ok()
                    }
                    && current(core, &job, now())
                    && store.bot_profile(&id).ok().as_ref() == Some(&job.profile);
                if valid { actions.jobs.insert(id.clone(), job.clone()); }
                if !valid {
                    job.cancel.store(true, Ordering::SeqCst);
                    let _ = actions.executor.fail(
                        &job.authority,
                        &job.key,
                        Failure::Interrupted,
                        now(),
                    );
                } else if status == Some(Status::Started) && job.visual.is_none() {
                    match store.load_life(&id) {
                        Ok(Some(snapshot)) => match actions.executor.tick_native_dwell(
                            &job.authority,
                            &job.key,
                            snapshot.state(),
                            now(),
                        ) {
                            Ok(Decision::Commit(request)) => {
                                if let Err(failure) = commit_request(
                                    actions, core, store, world, &job, request, false, &now, &wall,
                                ) {
                                    job.cancel.store(true, Ordering::SeqCst);
                                    let _ = actions.executor.fail(
                                        &job.authority,
                                        &job.key,
                                        failure,
                                        now(),
                                    );
                                    if failure == Failure::Storage {
                                        core.mark_storage_failure(&id, now());
                                    }
                                }
                            }
                            Ok(_) => (),
                            Err(_) => {
                                let _ = actions.executor.fail(
                                    &job.authority,
                                    &job.key,
                                    Failure::Protocol,
                                    now(),
                                );
                            }
                        },
                        _ => {
                            let _ = actions.executor.fail(
                                &job.authority,
                                &job.key,
                                Failure::Storage,
                                now(),
                            );
                            core.mark_storage_failure(&id, now());
                        }
                    }
                }
            }
        }
        // Cleanup is identity-bound. Cancellation needs no stale output/world grant,
        // and can still persist after Stop already removed the Core session.
        for _ in 0..ex::MAX_JOBS {
            let Some(cleanup) = actions.executor.take_cleanup() else {
                break;
            };
            let Some(job) = actions
                .jobs
                .get(cleanup.key.actor().as_str())
                .filter(|j| j.key == cleanup.key)
                .cloned()
            else {
                continue;
            };
            job.cancel.store(true, Ordering::SeqCst);
            if let Some(lease) = cleanup.lease {
                let _ = world.release(&job.world.session, &lease, now());
            }
            if let Some(reason) = cleanup.cancel_reason {
                match store.load_life(job.key.actor().as_str()) {
                    Ok(Some(snapshot)) => {
                        if snapshot.state().current_action().is_some_and(|a| {
                            a.intent().id() == job.key.action_id()
                                && a.phase() == crate::life_domain::ActionPhase::Cancelled
                        }) {
                            let _ = actions.executor.confirm_native_cancelled(
                                &job.authority,
                                &job.key,
                                snapshot.state(),
                                now(),
                            );
                            replace(core, &job, &snapshot);
                        } else if let Ok(Decision::Commit(request)) = actions
                            .executor
                            .cancellation_commit(&job.authority, &job.key, snapshot.state(), reason)
                        {
                            if commit_request(
                                actions, core, store, world, &job, request, true, &now, &wall,
                            )
                            .is_err()
                            {
                                core.mark_storage_failure(job.key.actor().as_str(), now());
                            }
                        }
                    }
                    _ => core.mark_storage_failure(job.key.actor().as_str(), now()),
                }
            }
        }
        flush_visual_dispatch(actions, core, now());
        let committed = actions
            .jobs
            .iter()
            .filter_map(|(id, job)| {
                let snapshot = actions.executor.snapshot(&job.key)?;
                (snapshot.terminal_committed
                    && matches!(
                        snapshot.status,
                        Status::Completed | Status::Cancelled | Status::Failed
                    ))
                .then(|| (id.clone(), snapshot))
            })
            .collect::<Vec<_>>();
        for (id, snapshot) in committed {
            actions.forget(&id);
            if !actions.jobs.contains_key(&id) {
                actions
                    .retired
                    .retain(|s| s.action_id.actor_id().as_str() != id);
                if actions.retired.len() >= ex::MAX_JOBS {
                    actions.retired.pop_front();
                }
                actions.retired.push_back(snapshot);
            }
        }
        // A permanently failed optional Store must not consume a global slot
        // forever. Revoke exact native session+World+timer first; retain only a
        // bounded public diagnostic, never failed authority or a reusable permit.
        let retire = actions
            .jobs
            .iter()
            .filter_map(|(id, job)| {
                let snapshot = actions.executor.snapshot(&job.key)?;
                (matches!(snapshot.status, Status::Failed | Status::Cancelled)
                    && !snapshot.terminal_committed)
                    .then(|| (id.clone(), job.clone(), snapshot))
            })
            .collect::<Vec<_>>();
        for (id, job, snapshot) in retire {
            job.cancel.store(true, Ordering::SeqCst);
            if core
                .sessions
                .get(&id)
                .is_some_and(|s| s.session_id == job.fence.session)
            {
                core.stop(&id, now());
            }
            if core
                .sessions
                .get(&id)
                .is_some_and(|s| s.session_id == job.fence.session)
            {
                continue;
            }
            if world.revoke_action_context(&job.world).is_err() {
                continue;
            }
            if job.visual.is_some() {
                core.world_outputs.cancel_actor(Some(&id));
            }
            let retired = if let Some(visual) = &job.visual {
                if !visual.output.cancel.load(Ordering::SeqCst) {
                    continue;
                }
                actions
                    .executor
                    .retire_uncommitted_visual(&job.authority, &job.key)
            } else {
                actions
                    .executor
                    .retire_uncommitted_native_rest(&job.authority, &job.key)
            };
            if retired.is_ok() {
                actions.jobs.remove(&id);
                actions
                    .retired
                    .retain(|s| s.action_id.actor_id().as_str() != id);
                if actions.retired.len() >= ex::MAX_JOBS {
                    actions.retired.pop_front();
                }
                actions.retired.push_back(snapshot);
            }
        }
        // #72 has no real adapter yet. NativeRest has no Offer; consume internal
        // terminal/control messages without misrouting them to speech/legacy outputs.
        flush_visual_dispatch(actions, core, now());
        while let Some(output) = actions.executor.take_invalidated_output() {
            for job in actions.jobs.values() {
                if job
                    .visual
                    .as_ref()
                    .is_some_and(|v| v.output.stamp == output)
                {
                    job.cancel.store(true, Ordering::SeqCst);
                    if let Some(v) = &job.visual {
                        v.output.cancel.store(true, Ordering::SeqCst);
                    }
                }
            }
        }
        actions.changed();
        Ok(())
    })?;
    world.synchronize(store, &world_hub::current_actors(core), None, now())
}

pub(crate) fn status(
    core: &Core,
    store: &Store,
    world: &WorldHub,
    bot: &str,
    now: u64,
) -> Result<Value, String> {
    let profile = store.bot_profile(bot).map_err(invalid)?;
    let snapshot = store.load_life(bot);
    let (confirmed, availability, reason) = match &snapshot {
        Ok(Some(snapshot)) => {
            let confirmed=snapshot.state().current_action().map(|a|json!({"actionId":a.intent().id(),"activity":a.intent().activity(),"phase":a.phase(),"cancellation":a.cancellation(),"revision":snapshot.state().revision()}));
            let reason = if profile["output"]["mode"] == "obs" {
                Some("output_unavailable")
            } else if world.snapshot_for(bot).is_none() {
                Some("world_unavailable")
            } else {
                core.local_action_reason(bot, now)
            };
            (
                confirmed,
                if reason.is_none() {
                    "ready"
                } else {
                    "unavailable"
                },
                reason,
            )
        }
        Ok(None) => (None, "not_initialized", Some("life_not_initialized")),
        Err(_) => (None, "unavailable", Some("storage_unavailable")),
    };
    let actions = core.actions.as_ref().ok_or(UNAVAILABLE)?;
    let runtime=actions.jobs.get(bot).and_then(|job|actions.executor.snapshot(&job.key)).or_else(||actions.retired.iter().rev().find(|s|s.action_id.actor_id().as_str()==bot).cloned()).map(|s|json!({"actionId":s.action_id,"presentation":s.presentation,"status":s.status,"failure":s.failure,"activeMilliseconds":s.active_ms}));
    Ok(
        json!({"version":1,"instanceId":actions.instance_id,"sequence":actions.sequence,"botId":bot,"availability":availability,"reason":reason,"visualAvailable":core.world_output_current(bot,now) && (core.world_outputs.source(bot).is_some_and(|source|matches!(source.route,crate::world_output::SourceRoute::Native(_))) || profile["output"]["mode"].as_str().unwrap_or("studio")=="studio" && store.profile().ok().is_some_and(|p|p["id"]==bot)),"confirmed":confirmed,"runtime":runtime}),
    )
}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;

/// Real checked Plan for selected Studio MoveTo. The separate actual-native
/// entry below additionally requires its factory-owned NativeActionPermit.
pub(crate) fn admit_move(
    core: &mut Core,
    store: &Store,
    world: &WorldHub,
    bot: &str,
    profile: &Value,
    fence: LocalFence,
    context: ActionContext,
    output: crate::world_output::OutputProof,
    target: crate::world_state::coordinates::Point,
    cancel: Arc<AtomicBool>,
    now: impl Fn() -> u64,
    wall: impl Fn() -> u64,
) -> Result<(), String> {
    admit_visual_move(core, store, world, bot, profile, fence, context, output, Goal::Point(target), None, None, cancel, now, wall)
}
/// A native scene permit is required separately from slot geometry. Neither a
/// caller's slot ID nor a private layout observation grants this authority.
pub(crate) fn admit_slot_move(
    core: &mut Core, store: &Store, world: &WorldHub, bot: &str, profile: &Value,
    fence: LocalFence, context: ActionContext, output: crate::world_output::OutputProof,
    permit: super::native_scene::NativeActionPermit, slot: crate::world_state::SlotId,
    cancel: Arc<AtomicBool>, now: impl Fn() -> u64, wall: impl Fn() -> u64,
) -> Result<(), String> {
    if !permit.current(core, now()) || !permit.matches_output(&output) || !core.local_current(bot, &fence, now()) || cancel.load(Ordering::SeqCst) {
        return Err(UNAVAILABLE.into());
    }
    let target = world.slot_target(store, &context, &slot)?;
    let goal = Goal::Slot { id: target.id.clone(), kind: target.kind, position: target.position };
    admit_visual_move(core, store, world, bot, profile, fence, context, output, goal, Some(permit), Some(target), cancel, now, wall)
}
pub(crate) fn admit_native_move(
    core: &mut Core, store: &Store, world: &WorldHub, bot: &str, profile: &Value,
    fence: LocalFence, context: ActionContext, output: crate::world_output::OutputProof,
    permit: super::native_scene::NativeActionPermit, target: crate::world_state::coordinates::Point,
    cancel: Arc<AtomicBool>, now: impl Fn() -> u64, wall: impl Fn() -> u64,
) -> Result<(), String> {
    admit_visual_move(core, store, world, bot, profile, fence, context, output, Goal::Point(target), Some(permit), None, cancel, now, wall)
}
/// Exercises the shared slot transaction using an actual selected-Studio
/// registration. This exposes no product Slot route or native surface grant.
#[cfg(test)]
pub(super) fn admit_studio_slot_fixture(
    core: &mut Core, store: &Store, world: &WorldHub, bot: &str, profile: &Value,
    fence: LocalFence, context: ActionContext, output: crate::world_output::OutputProof,
    slot: crate::world_state::SlotId, cancel: Arc<AtomicBool>,
    now: impl Fn() -> u64, wall: impl Fn() -> u64,
) -> Result<(), String> {
    let target = world.slot_target(store, &context, &slot)?;
    let goal = Goal::Slot { id: target.id.clone(), kind: target.kind, position: target.position };
    admit_visual_move(core, store, world, bot, profile, fence, context, output, goal, None, Some(target), cancel, now, wall)
}
fn admit_visual_move(
    core: &mut Core, store: &Store, world: &WorldHub, bot: &str, profile: &Value,
    fence: LocalFence, context: ActionContext, output: crate::world_output::OutputProof,
    goal: Goal, native_permit: Option<super::native_scene::NativeActionPermit>,
    slot_target: Option<crate::world_hub::ActionSlotTarget>, cancel: Arc<AtomicBool>,
    now: impl Fn() -> u64, wall: impl Fn() -> u64,
) -> Result<(), String> {
    let target = match &goal { Goal::Point(point) | Goal::Slot { position: point, .. } => *point, _ => return Err(UNAVAILABLE.into()) };
    with_actions(core, |actions, core| {
        let actor = ActorId::new(bot).map_err(invalid)?;
        actions.forget(bot);
        if actions.actor_busy(bot)
            || !actions.executor.can_admit(&actor)
            || !core.local_current(bot, &fence, now())
            || cancel.load(Ordering::SeqCst)
            || match &native_permit {
                Some(permit) => !permit.current(core, now()) || !permit.matches_output(&output),
                None => !core.world_output_current(bot, now()),
            }
            || !core.world_outputs.check_proof(&output, now())
        {
            return Err(UNAVAILABLE.into());
        }
        let visual =
            crate::world_output::VisualExecution::new(output, target, now()).map_err(invalid)?;
        let authority = NativeAuthority::new(
            context.session.clone(),
            context.source_world.clone(),
            context.stage.clone(),
            context.config_revision,
            context.topology_revision,
            visual.output.stamp.clone(),
            visual.output.binding.asset_revision,
        )
        .map_err(invalid)?;
        let caps = Capabilities::new(authority.clone(), vec![(Activity::MoveTo, 0)], false)
            .map_err(invalid)?;
        let persisted_profile = store.action_profile_token(bot, profile).map_err(invalid)?;
        if persisted_profile != visual.output.profile_token {
            return Err(UNAVAILABLE.into());
        }
        let (commit, prepared) = world.with_action_context(store, &context, || {
            store
                .apply_life_action_checked(bot, wall(), |actual, tx| {
                    if cancel.load(Ordering::SeqCst)
                        || !core.local_current(bot, &fence, now())
                        || !core.world_outputs.check_proof(&visual.output, now())
                        || native_permit.as_ref().is_some_and(|permit| !permit.current(core, now()) || !permit.matches_output(&visual.output))
                        || !crate::storage::action_store::matches(
                            tx,
                            bot,
                            &persisted_profile,
                            &context.record,
                            native_permit.is_none(),
                        )
                    {
                        return Err(LifeStoreError::InvalidInput);
                    }
                    let plan = PreparedPlan::new(
                        authority.clone(),
                        actual.state(),
                        actual.random(),
                        Activity::MoveTo,
                        goal.clone(),
                        Presentation::Visual,
                        &caps,
                    )
                    .map_err(|_| LifeStoreError::InvalidInput)?;
                    Ok((plan.event().clone(), plan.expected_revision(), plan))
                })
                .map_err(invalid)
        })?;
        if commit.outcome != WriteOutcome::Applied {
            return Err(UNAVAILABLE.into());
        }
        let planned = commit.snapshot;
        core.replace_confirmed(
            bot,
            &fence.session,
            fence.revision,
            planned.state().clone(),
            None,
        );
        let slot = if let Some(target) = slot_target {
            match world.reserve_action_slot_current(core, store, &context, target, prepared.action_id().clone(), SLOT_LEASE_MS, &now) {
                Ok(slot) => Some(slot),
                Err(_) => {
                    cancel_uninstalled_move(core, store, world, bot, &fence, &planned, &now, &wall);
                    return Err(UNAVAILABLE.into());
                }
            }
        } else { None };
        if slot.as_ref().is_some_and(|slot| !slot.fresh(now()) || world.validate_action_slot_current(core, store, &context, slot, &now).is_err())
            || native_permit.as_ref().is_some_and(|permit| !permit.current(core, now()) || !permit.matches_output(&visual.output))
            || cancel.load(Ordering::SeqCst)
            || !core.local_current(bot, &fence, now())
            || !core.world_outputs.check_proof(&visual.output, now())
        {
            if let Some(slot) = &slot { let _ = world.release(&context.session, &slot.lease(), now()); }
            cancel_uninstalled_move(core, store, world, bot, &fence, &planned, &now, &wall);
            return Err(UNAVAILABLE.into());
        }
        let key = match actions
            .executor
            .install_plan(prepared, planned.state(), slot.as_ref().map(|slot| slot.lease()), now())
        {
            Ok(key) => key,
            Err(_) => {
                if let Some(slot) = &slot { let _ = world.release(&context.session, &slot.lease(), now()); }
                cancel_uninstalled_move(core, store, world, bot, &fence, &planned, &now, &wall);
                return Err(UNAVAILABLE.into());
            }
        };
        actions.jobs.insert(
            bot.into(),
            NativeJob {
                visual: Some(visual),
                native_rest_output: None,
                native_permit,
                slot,
                authority,
                key,
                fence,
                world: context,
                profile: profile.clone(),
                persisted_profile,
                cancel,
            },
        );
        flush_visual_dispatch(actions, core, now());
        actions.changed();
        Ok(())
    })
}
fn cancel_uninstalled_move(
    core: &mut Core, store: &Store, world: &WorldHub, bot: &str, fence: &LocalFence,
    planned: &LifeSnapshot, now: &impl Fn() -> u64, wall: &impl Fn() -> u64,
) {
    let Some(action) = planned.state().current_action().map(|action| action.intent().id().clone()) else {
        core.mark_storage_failure(bot, now()); world.revoke_actor(store, bot); return;
    };
    match store.apply_life_action_checked(bot, wall(), |actual, _| {
        if actual.state().current_action().is_none_or(|current| current.intent().id() != &action) {
            return Err(LifeStoreError::InvalidInput);
        }
        Ok((LifeEvent::Cancel { action_id: action.clone(), reason: CancelReason::Unavailable }, actual.state().revision(), ()))
    }) {
        Ok((commit, _)) => { core.replace_confirmed(bot, &fence.session, fence.revision, commit.snapshot.state().clone(), None); }
        Err(_) => { core.mark_storage_failure(bot, now()); world.revoke_actor(store, bot); }
    }
}
fn flush_visual_dispatch(actions: &mut NativeActions, core: &mut Core, now: u64) {
    while let Some(dispatch) = actions.executor.take_dispatch() {
        let (action, delivery) = match &dispatch.message {
            ex::Message::Offer {
                action_id,
                delivery_id,
                ..
            }
            | ex::Message::Pause {
                action_id,
                delivery_id,
            }
            | ex::Message::Resume {
                action_id,
                delivery_id,
            }
            | ex::Message::Terminal {
                action_id,
                delivery_id,
                ..
            } => (action_id.clone(), delivery_id.clone()),
        };
        let Some(job) = actions
            .jobs
            .get_mut(action.actor_id().as_str())
            .filter(|j| j.key.action_id() == &action)
        else {
            continue;
        };
        let Some(visual) = &mut job.visual else {
            continue;
        };
        if visual.output.stamp != dispatch.output {
            job.cancel.store(true, Ordering::SeqCst);
            continue;
        }
        let fence = visual
            .fence
            .clone()
            .unwrap_or_else(|| visual.fence(action.clone(), delivery.clone()));
        if fence.delivery_id != delivery {
            job.cancel.store(true, Ordering::SeqCst);
            continue;
        }
        visual.fence = Some(fence.clone());
        let command = match dispatch.message {
            ex::Message::Offer {
                activity: Activity::MoveTo,
                presentation: Presentation::Visual,
                ..
            } => crate::world_output::MotionCommand::Offer {
                version: 1,
                sequence: dispatch.sequence,
                fence,
                activity: "move_to",
                from: visual.from,
                target: visual.destination,
                maximum_speed: visual.maximum_speed,
                acceleration: visual.acceleration,
                valid_for_ms: 30_000,
            },
            ex::Message::Pause { .. } => crate::world_output::MotionCommand::Pause {
                version: 1,
                sequence: dispatch.sequence,
                fence,
            },
            ex::Message::Resume { .. } => crate::world_output::MotionCommand::Resume {
                version: 1,
                sequence: dispatch.sequence,
                fence,
            },
            ex::Message::Terminal { status, .. } => {
                core.world_outputs.record_terminal(
                    fence.clone(),
                    status,
                    visual.last_receipt,
                    visual.last_hash,
                );
                crate::world_output::MotionCommand::Terminal {
                    version: 1,
                    sequence: dispatch.sequence,
                    fence,
                    status,
                }
            }
            _ => {
                job.cancel.store(true, Ordering::SeqCst);
                continue;
            }
        };
        if core.world_outputs.enqueue(command).is_err() {
            job.cancel.store(true, Ordering::SeqCst);
            visual.output.cancel.store(true, Ordering::SeqCst);
            let _ = actions
                .executor
                .fail(&job.authority, &job.key, Failure::OutputLost, now);
        }
    }
}
/// Caller authenticates current main attachment before entering this owner.
/// Frame arrival time is captured BEFORE any Main/Core/SQLite wait.
pub(crate) fn visual_report(
    core: &mut Core,
    store: &Store,
    world: &WorldHub,
    report: &crate::world_output::ActionReport,
    received: u64,
    now: impl Fn() -> u64,
    wall: impl Fn() -> u64,
) -> Result<crate::world_output::ActionReportAck, String> {
    use crate::world_output::{ActionReportAck, RenderReason, ReportKind, ReportOutcome};
    if !report.valid() {
        return Err(UNSUPPORTED.into());
    }
    let (_, fence, sequence, frame, kind) = report.parts();
    use sha2::{Digest, Sha256};
    let report_hash: [u8; 32] =
        Sha256::digest(serde_json::to_vec(report).map_err(|_| UNAVAILABLE)?).into();
    with_actions(core, |actions, core| {
        if let Some(status) = core
            .world_outputs
            .terminal_duplicate(fence, sequence, &report_hash)
        {
            let phase = store
                .load_life(&fence.actor_id)
                .ok()
                .flatten()
                .and_then(|life| {
                    life.state()
                        .current_action()
                        .filter(|a| a.intent().id() == &fence.action_id)
                        .map(|a| a.phase())
                });
            return Ok(ActionReportAck {
                version: 1,
                receipt_sequence: sequence,
                outcome: ReportOutcome::Duplicate,
                runtime_status: Some(status),
                confirmed_phase: phase,
                reason: None,
            });
        }
        let mut job = actions
            .jobs
            .get(&fence.actor_id)
            .filter(|job| job.key.action_id() == &fence.action_id)
            .cloned()
            .ok_or(UNAVAILABLE)?;
        let mut visual = job
            .visual
            .clone()
            .filter(|v| v.fence.as_ref() == Some(fence))
            .ok_or(UNAVAILABLE)?;
        let current_phase = |store: &Store| {
            store
                .load_life(&fence.actor_id)
                .ok()
                .flatten()
                .and_then(|life| life.state().current_action().map(|a| a.phase()))
        };
        if sequence < visual.last_receipt
            || (sequence == visual.last_receipt && visual.last_hash != Some(report_hash))
        {
            return Err(UNAVAILABLE.into());
        }
        if sequence == visual.last_receipt {
            return Ok(ActionReportAck {
                version: 1,
                receipt_sequence: sequence,
                outcome: ReportOutcome::Duplicate,
                runtime_status: actions.executor.status(&job.key),
                confirmed_phase: current_phase(store),
                reason: None,
            });
        }
        let attempt = (|| -> Result<(), Failure> {
            if !current(core, &job, now())
                || store.bot_profile(&fence.actor_id).ok().as_ref() != Some(&job.profile)
            {
                return Err(Failure::Interrupted);
            }
            core.world_outputs
                .check_frame(&visual.output, frame, now())
                .map_err(|_| Failure::Protocol)?;
            let position = visual
                .observe(frame, kind, received, now())
                .map_err(|_| Failure::Protocol)?;
            if let Some(slot) = &mut job.slot {
                world.renew_action_slot_current(core, store, &job.world, slot, SLOT_LEASE_MS, &now)
                    .map_err(|_| Failure::Interrupted)?;
            }
            let before = store
                .load_life(&fence.actor_id)
                .map_err(|_| Failure::Storage)?
                .ok_or(Failure::Storage)?;
            if !visual.fresh(now()) || !current(core, &job, now()) {
                return Err(Failure::Interrupted);
            }
            if let Some(slot) = &job.slot {
                world.with_action_slot_context(store, &job.world, slot, &now, || {
                    if current(core, &job, now()) && visual.fresh(now()) { Ok(()) } else { Err(UNAVAILABLE.into()) }
                }).map_err(|_| Failure::Interrupted)?;
            }
            let status = actions.executor.status(&job.key);
            if !matches!(
                (kind, status),
                (ReportKind::Delivered, Some(Status::AwaitingDelivery))
                    | (ReportKind::Started, Some(Status::Delivered))
                    | (
                        ReportKind::Progress | ReportKind::Completed,
                        Some(Status::Started)
                    )
            ) {
                return Err(Failure::Protocol);
            }
            visual.last_receipt = sequence;
            visual.last_hash = Some(report_hash);
            job.visual = Some(visual.clone());
            actions.jobs.insert(fence.actor_id.clone(), job.clone());
            let receipt = match kind {
                ReportKind::Delivered => ex::Receipt::Delivered,
                ReportKind::Started => ex::Receipt::Started,
                ReportKind::Progress => ex::Receipt::Progress,
                ReportKind::Completed => ex::Receipt::Completed(ex::Evidence::Arrived(position)),
            };
            let decision = actions
                .executor
                .receipt(
                    &job.authority,
                    &job.key,
                    &fence.delivery_id,
                    sequence,
                    receipt,
                    before.state(),
                    now(),
                )
                .map_err(|_| Failure::Protocol)?;
            if actions
                .executor
                .status(&job.key)
                .is_some_and(|s| matches!(s, Status::Cancelled | Status::Failed))
            {
                return Err(Failure::Interrupted);
            }
            // Start's genuine Store commit precedes accepting its first displacement.
            if kind == ReportKind::Started {
                if let Decision::Commit(request) = decision.clone() {
                    commit_request(
                        actions, core, store, world, &job, request, false, &now, &wall,
                    )?;
                }
            }
            if kind != ReportKind::Delivered {
                job.world = if let Some(slot) = &job.slot {
                    world.apply_slot_progress(store, &job.world, slot, position, &now, ||
                        current(core, &job, now()) && visual.fresh(now()))
                } else {
                    world.apply_visual_progress(store, &job.world, position, ||
                        current(core, &job, now()) && visual.fresh(now()))
                }.map_err(|_| Failure::Interrupted)?;
            }
            core.world_outputs
                .accept_frame(&visual.output, frame, position, received)
                .map_err(|_| Failure::Protocol)?;
            actions.jobs.insert(fence.actor_id.clone(), job.clone());
            if kind == ReportKind::Completed {
                if let Decision::Commit(request) = decision {
                    commit_request(
                        actions, core, store, world, &job, request, false, &now, &wall,
                    )?;
                }
            }
            Ok(())
        })();
        let (reason, outcome) = if let Err(failure) = attempt {
            job.cancel.store(true, Ordering::SeqCst);
            let _ = actions
                .executor
                .fail(&job.authority, &job.key, failure, now());
            if failure == Failure::Storage {
                core.mark_storage_failure(&fence.actor_id, now());
            }
            (
                Some(if failure == Failure::Storage {
                    RenderReason::StorageUnavailable
                } else {
                    RenderReason::Cancelled
                }),
                ReportOutcome::Cancelled,
            )
        } else {
            (None, ReportOutcome::Accepted)
        };
        flush_visual_dispatch(actions, core, now());
        actions.changed();
        Ok(ActionReportAck {
            version: 1,
            receipt_sequence: sequence,
            outcome,
            runtime_status: actions.executor.status(&job.key),
            confirmed_phase: current_phase(store),
            reason,
        })
    })
}
