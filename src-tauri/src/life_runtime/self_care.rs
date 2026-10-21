//! One native-loop self-care chooser per explicitly started actor session.
//! An offer never bypasses the existing surface, World lease or executor gates.
use super::{action_host, LifeRuntime};
use crate::{
    life_domain::{ActionId, ActionPhase, Activity},
    life_self_care::{CareConfig, Chooser, End, Need, Offer, OfferKey, Target},
    world_state::AnchorKind,
};
use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use tauri::Manager;

struct Entry {
    session: String,
    world_context: (String, u64, u64),
    config: CareConfig,
    config_revision: u64,
    home: crate::world_state::coordinates::Point,
    chooser: Chooser,
    pending: Option<(ActionId, OfferKey)>,
    package: Option<String>,
    retry_after: u64,
}
#[derive(Default)]
pub(super) struct CareMemory {
    entries: Mutex<BTreeMap<String, Entry>>,
}
impl CareMemory {
    pub(super) fn package_actors(&self, package: &str) -> Result<Vec<String>, String> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| "experience_native_unavailable")?
            .iter()
            .filter(|(_, entry)| {
                entry.pending.is_some() && entry.package.as_deref() == Some(package)
            })
            .map(|(bot, _)| bot.clone())
            .collect())
    }
}

fn need(kind: AnchorKind) -> Need {
    match kind {
        AnchorKind::Rest => Need::Energy,
        AnchorKind::Snack => Need::Nourishment,
        AnchorKind::Play => Need::Engagement,
        AnchorKind::Meeting => Need::Social,
    }
}
fn activity(need: Need) -> Activity {
    match need {
        Need::Energy => Activity::Rest,
        Need::Nourishment => Activity::Snack,
        Need::Engagement => Activity::Play,
        Need::Social => Activity::Socialize,
    }
}

pub(super) fn pulse(runtime: &LifeRuntime, app: &tauri::AppHandle) {
    let state = app.state::<crate::AppState>();
    let experience = app.state::<crate::experience::ExperienceRuntime>();
    if runtime.stopping.load(std::sync::atomic::Ordering::SeqCst)
        || state.updater.ensure_idle().is_err()
        || state.overlay.has_any_pending_speech()
    {
        return;
    }
    let Ok(_generation) = state.generation.try_lock() else {
        return;
    };
    let members = runtime.native_scenes.members(None);
    let Ok(mut memories) = runtime.care.entries.lock() else {
        return;
    };
    memories.retain(|bot, _| {
        members
            .iter()
            .any(|member| member.constructor.bot() == bot.as_str())
    });
    for member in members {
        let bot = member.constructor.bot();
        let now = runtime.now();
        if experience.paused(bot).unwrap_or(true) {
            continue;
        }
        let Ok(preferences) = state
            .store
            .experience_settings(bot)
            .map(|settings| settings.preferences)
        else {
            continue;
        };
        if !preferences.allowed_animations.iter().any(|a| a == "idle") {
            continue;
        }
        if !member.current()
            || !runtime
                .session_outputs
                .constructor_current(&member.constructor)
        {
            continue;
        }
        let Ok(owned_guard) = member.owned.lock() else {
            continue;
        };
        let Some(owned) = owned_guard.as_ref() else {
            continue;
        };
        let Ok(mut core) = runtime.core.lock() else {
            continue;
        };
        if !owned.context.current(&core, &state, now) {
            continue;
        }
        let Some(fence) = core.local_fence(bot, now) else {
            continue;
        };
        let Some(confirmed) = core.sessions.get(bot).and_then(|s| s.state.clone()) else {
            continue;
        };
        let Ok(settings) = state.store.life_care_preferences(bot) else {
            continue;
        };
        let Ok(source) =
            owned
                .context
                .source(&core, &state, owned.route.clone(), owned.framing, now)
        else {
            continue;
        };
        if !core.world_outputs.matches(bot, &source) {
            continue;
        }
        let Ok(output) = core.world_outputs.ready(bot, now) else {
            continue;
        };
        let Some(permit) = owned.action_permit(now) else {
            continue;
        };
        if !permit.current(&core, now) || !permit.matches_output(&output) {
            continue;
        }
        let context_key = (
            source.context.instance_id.clone(),
            source.context.config_revision,
            source.context.topology_revision,
        );
        let replace = memories.get(bot).is_none_or(|entry| {
            entry.session != fence.session
                || entry.world_context != context_key
                || entry.config_revision != settings.revision
                || entry.config != settings.config
        });
        if replace {
            let Ok(chooser) = settings.config.chooser(&confirmed) else {
                continue;
            };
            memories.insert(
                bot.into(),
                Entry {
                    session: fence.session.clone(),
                    world_context: context_key,
                    config: settings.config.clone(),
                    config_revision: settings.revision,
                    home: source.context.position,
                    chooser,
                    pending: None,
                    package: None,
                    retry_after: 0,
                },
            );
        }
        let Some(entry) = memories.get_mut(bot) else {
            continue;
        };
        let policy_now = confirmed.simulation_time().milliseconds();
        if let Some((id, key)) = &entry.pending {
            match confirmed
                .current_action()
                .filter(|current| current.intent().id() == id)
            {
                Some(current) if current.phase() == ActionPhase::Completed => {
                    let _ = entry.chooser.record_end(policy_now, key, End::Finished);
                    entry.pending = None;
                }
                Some(current) if current.phase() == ActionPhase::Cancelled => {
                    let _ = entry.chooser.record_end(policy_now, key, End::Interrupted);
                    entry.pending = None;
                }
                Some(_) => continue,
                None => {
                    let _ = entry.chooser.record_end(policy_now, key, End::Interrupted);
                    entry.pending = None;
                }
            }
        }
        if now < entry.retry_after
            || core
                .actions
                .as_ref()
                .is_none_or(|actions| actions.actor_busy(bot))
        {
            continue;
        }
        let Ok(slots) = state.world.action_slots(&state.store, &source.context) else {
            continue;
        };
        let mut offers = Vec::new();
        for wanted in [
            Need::Energy,
            Need::Nourishment,
            Need::Engagement,
            Need::Social,
        ] {
            let mut candidates = slots
                .iter()
                .filter(|slot| {
                    need(slot.kind) == wanted
                        && preferences.allowed_animations.iter().any(|a| a == "walk")
                        && within_radius(entry.home, slot.position, preferences.movement_radius)
                })
                .map(|slot| {
                    let cost = (slot.position.x().abs_diff(source.context.position.x()) as u64
                        + slot.position.y().abs_diff(source.context.position.y()) as u64)
                        / 2000;
                    (cost.min(1000) as u16, slot)
                })
                .collect::<Vec<_>>();
            candidates.sort_by(|(cost_a, a), (cost_b, b)| cost_a.cmp(cost_b).then(a.id.cmp(&b.id)));
            if candidates.is_empty() {
                if let Ok(offer) = Offer::new(wanted, Target::Here, 1000) {
                    offers.push(offer);
                }
            } else {
                for (cost, slot) in candidates.into_iter().take(8) {
                    if let Ok(offer) = Offer::new(wanted, Target::Slot(slot.id.clone()), cost) {
                        offers.push(offer);
                    }
                }
            }
        }
        let Ok(Some(proposal)) = entry.chooser.choose(policy_now, &confirmed, &offers, None) else {
            continue;
        };
        let slot = match proposal.offer().target() {
            Target::Slot(id) => Some(id.clone()),
            Target::Here => None,
            Target::Actor(_) => continue,
        };
        let key = proposal.offer().key().clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let Some(ticket) = runtime.outputs.ticket() else {
            continue;
        };
        if !runtime.outputs.watch_delivery(
            ticket,
            bot,
            member.constructor.window().label(),
            &cancel,
        ) || runtime
            .monitor
            .get()
            .and_then(|monitor| monitor.gate.watch(&cancel))
            .is_none()
            || runtime
                .session_outputs
                .bind_surface(&member.constructor, &cancel)
                .is_err()
        {
            continue;
        }
        let result = action_host::admit_native_care(
            &mut core,
            &state.store,
            &state.world,
            bot,
            &owned.context.profile,
            fence,
            source.context,
            output,
            permit,
            activity(proposal.offer().need()),
            slot,
            cancel,
            || runtime.now(),
            super::wall_ms,
        );
        if result.is_ok() {
            if let Some(id) = core
                .actions
                .as_ref()
                .and_then(|actions| actions.jobs.get(bot))
                .map(|job| job.key.action_id().clone())
            {
                entry.pending = Some((id, key));
            }
        } else {
            let _ = entry.chooser.record_end(policy_now, &key, End::Unavailable);
            entry.retry_after = now.saturating_add(2000);
        }
    }
}

fn within_radius(
    home: crate::world_state::coordinates::Point,
    target: crate::world_state::coordinates::Point,
    radius: u16,
) -> bool {
    let dx = u64::from(home.x().abs_diff(target.x()));
    let dy = u64::from(home.y().abs_diff(target.y()));
    let limit = u64::from(radius.min(1000)) * 1000;
    dx * dx + dy * dy <= limit * limit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_state::coordinates::Point;
    #[test]
    fn movement_radius_stays_anchored_and_zero_allows_only_here() {
        let home = Point::new(300_000, 300_000).unwrap();
        assert!(within_radius(home, home, 0));
        assert!(!within_radius(
            home,
            Point::new(300_001, 300_000).unwrap(),
            0
        ));
        assert!(within_radius(
            home,
            Point::new(450_000, 500_000).unwrap(),
            250
        ));
        assert!(!within_radius(
            home,
            Point::new(450_001, 500_000).unwrap(),
            250
        ));
    }
}
