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
/// Visual dispatch remains explicitly unsupported until #72's registered output.
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
        let authority = NativeAuthority::new(
            context.session.clone(),
            context.source_world.clone(),
            context.stage.clone(),
            context.config_revision,
            context.topology_revision,
            OutputStamp::new(format!("native-rest-{}", fence.session), fence.revision)
                .map_err(invalid)?,
            fence.revision,
        )
        .map_err(invalid)?;
        let caps = Capabilities::new(authority.clone(), vec![], true).map_err(invalid)?;
        let persisted_profile = store.action_profile_token(bot, profile).map_err(invalid)?;
        let (commit, prepared) = world.with_action_context(store, &context, || {
            if store.bot_profile(bot).map_err(invalid)? != *profile {
                return Err(UNAVAILABLE.into());
            }
            store
                .apply_life_action_checked(bot, wall(), |actual, tx| {
                    if cancel.load(Ordering::SeqCst)
                        || !core.local_current(bot, &fence, now())
                        || !crate::storage::action_store::matches(
                            tx,
                            bot,
                            &persisted_profile,
                            &context.record,
                            profile["output"]["mode"] != "overlay",
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
                        || !crate::storage::action_store::matches(
                            tx,
                            bot,
                            &job.persisted_profile,
                            &job.world.record,
                            job.profile["output"]["mode"] != "overlay",
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
        world
            .with_action_context(store, &job.world, || Ok(operation()))
            .map_err(|_| Failure::Interrupted)?
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
            let Some(job) = actions.jobs.get(&id).cloned() else {
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
                    && world
                        .with_action_context(store, &job.world, || Ok(()))
                        .is_ok();
                if !valid {
                    job.cancel.store(true, Ordering::SeqCst);
                    let _ = actions.executor.fail(
                        &job.authority,
                        &job.key,
                        Failure::Interrupted,
                        now(),
                    );
                } else if status == Some(Status::Started) {
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
            if actions
                .executor
                .retire_uncommitted_native_rest(&job.authority, &job.key)
                .is_ok()
            {
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
        while actions.executor.take_dispatch().is_some() {}
        while actions.executor.take_invalidated_output().is_some() {}
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
        json!({"version":1,"instanceId":actions.instance_id,"sequence":actions.sequence,"botId":bot,"availability":availability,"reason":reason,"visualAvailable":false,"confirmed":confirmed,"runtime":runtime}),
    )
}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;
