//! One native-loop self-care chooser per explicitly started actor session.
//! An offer never bypasses the existing surface, World lease or executor gates.
use super::{action_host, LifeRuntime};
use crate::{
    character_development::HobbyKind,
    life_domain::{ActionId, ActionPhase, Activity},
    life_self_care::{CareConfig, Chooser, End, Need, Offer, OfferKey, Target},
    world_state::AnchorKind,
};
use chrono::Timelike;
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
    retry_after: u64,
}
#[derive(Default)]
pub(super) struct CareMemory {
    entries: Mutex<BTreeMap<String, Entry>>,
}
impl CareMemory {
    pub(super) fn origin(&self, bot: &str) -> Option<(String, (String, u64, u64), crate::world_state::coordinates::Point)> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(bot)?;
        Some((entry.session.clone(), entry.world_context.clone(), entry.home))
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

/// The elapsed clock keeps event ordering stable; routines still follow the
/// operator's local day (including the offset at that anchored instant).
fn routine_minute<T: chrono::TimeZone>(epoch_ms: u64, zone: &T) -> Option<u16> {
    let instant = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(i64::try_from(epoch_ms).ok()?)?;
    let local = instant.with_timezone(zone);
    Some((local.hour() * 60 + local.minute()) as u16)
}

fn same_goal_world(a: &crate::world_output::MotionBinding, b: &crate::world_output::MotionBinding) -> bool {
    a.world_instance_id == b.world_instance_id && a.stage_id == b.stage_id
        && a.config_revision == b.config_revision && a.topology_revision == b.topology_revision
}

/// Read an existing partner through Core's retained registry, never another
/// member's Owned lock. These observations are goal hints, not action permits.
fn current_company(
    core: &super::core::Core,
    store: &crate::storage::Store,
    experience: &crate::experience::ExperienceRuntime,
    output: &crate::world_output::OutputProof,
    now: u64,
) -> bool {
    core.sessions.iter().any(|(bot, session)| {
        if bot == &output.binding.actor_id || session.mode != super::core::Mode::Life
            || session.storage_failed || !session.output || experience.paused(bot).unwrap_or(true)
            || core.local_fence(bot, now).is_none() { return false; }
        let Some(life) = &session.state else { return false; };
        let Ok(partner) = core.world_outputs.ready(bot, now) else { return false; };
        if !matches!(&partner.route, crate::world_output::SourceRoute::Native(route) if route.context_current(core, now))
            || !same_goal_world(&output.binding, &partner.binding)
            || partner.binding.actor_id != life.actor_id().as_str()
            || partner.binding.source_world_id != life.world_id().as_str()
            || !partner.binding.station_supported { return false; }
        let Ok(settings) = store.experience_settings(bot) else { return false; };
        if !["idle", "walk"].iter().all(|motion|settings.preferences.allowed_animations.iter().any(|allowed|allowed.as_str()==*motion)) {
            return false;
        }
        store.bot_profile(bot).ok().is_some_and(|profile|
            store.action_profile_token(bot, &profile).ok() == Some(partner.profile_token))
    })
}

fn goal_options(offers: &[Offer], walk_target: bool, company: bool) -> Vec<HobbyKind> {
    let mut available = Vec::new();
    if offers.iter().any(|offer|offer.need()==Need::Engagement) { available.push(HobbyKind::Play); }
    if walk_target { available.push(HobbyKind::Explore); }
    if company { available.push(HobbyKind::Company); }
    available
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
        if experience.paused(bot).unwrap_or(true) || runtime.social.busy(bot) || runtime.desktop.busy(bot) {
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
        // Native admission owns Core, not the dialog generation turn. Keeping
        // that lock would make Live-owner cleanup cancel this same surface.
        if state.generation.try_lock().is_err() {
            continue;
        }
        let Ok(mut core) = runtime.core.lock() else {
            continue;
        };
        if state.generation.try_lock().is_err() {
            continue;
        }
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
        let Ok(packs)=state.store.content_packs() else {continue;};
        let walk_allowed = preferences.allowed_animations.iter().any(|a|a=="walk");
        let target_fits = |point| output.binding.safe_centers.contains(crate::world_output::css(point, output.binding.viewport));
        let walk_target = walk_allowed && slots.iter().any(|slot|
            slot.position != source.context.position
                && within_radius(entry.home, slot.position, preferences.movement_radius)
                && target_fits(slot.position));
        let mut offers = Vec::new();
        for wanted in [
            Need::Energy,
            Need::Nourishment,
            Need::Engagement,
            Need::Social,
        ] {
            // The current adapter renders these activities through real visible
            // idle dwell. A saved animation name alone supplies no capability.
            if !output.binding.station_supported { continue; }
            let mut candidates = slots
                .iter()
                .filter_map(|slot| {
                    if need(slot.kind)!=wanted || !walk_allowed
                        || !within_radius(entry.home,slot.position,preferences.movement_radius)
                        || !target_fits(slot.position) {return None;}
                    let weight=if let Some(reference)=&slot.content_ref {
                        super::content_host::resolve_for_state(&packs,reference,slot.kind,activity(wanted),&confirmed,&settings.config)?.behavior.map_or(100,|behavior|behavior.weight)
                    } else {100};
                    let cost=(slot.position.x().abs_diff(source.context.position.x())as u64
                        +slot.position.y().abs_diff(source.context.position.y())as u64)/2000;
                    Some((cost.min(1000)as u16,slot,weight))
                })
                .collect::<Vec<_>>();
            candidates.sort_by(|(cost_a,a,_),(cost_b,b,_)|cost_a.cmp(cost_b).then(a.id.cmp(&b.id)));
            if candidates.is_empty() {
                if let Ok(offer) = Offer::new(wanted, Target::Here, 1000) {
                    offers.push(offer);
                }
            } else {
                for (cost, slot,weight) in candidates.into_iter().take(8) {
                    if let Ok(offer) = Offer::new(wanted, Target::Slot(slot.id.clone()), cost).and_then(|offer|offer.weighted(weight)) {offers.push(offer);}

                }
            }
        }
        let company = walk_allowed && output.binding.station_supported
            && current_company(&core, &state.store, &experience, &output, now);
        let available = goal_options(&offers, walk_target, company);
        // Core -> Store, no await or Core re-entry. Persist only advisory goal
        // status/selection, never a session, action, relationship or completion.
        let preferences = state.store.social_reconcile_goal(bot, &available)
            .and_then(|_|state.store.social_snapshot(bot)).ok()
            .zip(routine_minute(runtime.social_now(), &chrono::Local))
            .map(|(snapshot, minute)|character_preferences(&settings.config, &snapshot.development, minute))
            .unwrap_or_else(|| settings.config.preferences());
        if let Ok(preferences) = preferences { entry.chooser.update_preferences(preferences); }
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

fn character_preferences(config: &CareConfig, character: &crate::character_development::Character, minute: u16) -> Result<crate::life_self_care::Preferences, crate::life_self_care::Error> {
    use crate::character_development::{HobbyKind,GoalStatus};
    let mut data = config.clone();
    let factor = |value:u16, preference:u16| ((u32::from(value)*(500+u32::from(preference)))/1000).clamp(1,1000) as u16;
    data.priorities[0]=factor(data.priorities[0], character.traits.rest_preference);
    data.priorities[2]=factor(data.priorities[2], character.preference(HobbyKind::Play,minute));
    let company=((u32::from(character.traits.sociability)+u32::from(character.preference(HobbyKind::Company,minute)))/2) as u16;
    data.priorities[3]=factor(data.priorities[3],company);
    if let Some(goal)=character.goal.as_ref().filter(|goal|goal.status==GoalStatus::Active) {
        let index=match goal.kind {HobbyKind::Play=>Some(2),HobbyKind::Company=>Some(3),HobbyKind::Explore=>None};
        if let Some(index)=index {data.priorities[index]=((u32::from(data.priorities[index])*5)/4).min(1000) as u16;}
    }
    data.preferences()
}

pub(super) fn within_radius(
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

    #[test]
    fn routines_use_the_injected_local_offset_and_wrap_the_local_day() {
        use chrono::TimeZone;
        let at=chrono::Utc.with_ymd_and_hms(2026,10,21,23,30,0).unwrap().timestamp_millis() as u64;
        let east=chrono::FixedOffset::east_opt(2*3600).unwrap();
        let west=chrono::FixedOffset::west_opt(5*3600).unwrap();
        assert_eq!(routine_minute(at,&east),Some(90));
        assert_eq!(routine_minute(at,&west),Some(18*60+30));
        let winter=chrono::FixedOffset::east_opt(3600).unwrap();
        assert_eq!(routine_minute(at,&winter),Some(30));
        assert_eq!(routine_minute(at+30*60_000,&west),Some(19*60));
        assert!(routine_minute(u64::MAX,&east).is_none());
    }

    #[test]
    fn local_routine_time_changes_a_real_chooser_proposal_in_the_same_environment() {
        use chrono::TimeZone;
        use crate::{character_development::{Character,Routine},life_domain::{ActorId,LifeState,Needs,WorldId}};
        let at=chrono::Utc.with_ymd_and_hms(2026,10,21,7,30,0).unwrap().timestamp_millis() as u64;
        let local=chrono::FixedOffset::east_opt(2*3600).unwrap();
        let mut character=Character::default();
        character.routines.push(Routine{start_minute:9*60,end_minute:10*60,kind:HobbyKind::Play,weight:500});
        let config=CareConfig::default();
        let state=LifeState::new(ActorId::new("nova").unwrap(),WorldId::new("world_nova").unwrap(),Needs::new(5000,10000,5000,10000).unwrap());
        let offers=vec![Offer::new(Need::Energy,Target::Here,0).unwrap(),Offer::new(Need::Engagement,Target::Here,0).unwrap()];
        let choose=|minute| {
            let mut chooser=Chooser::new(&state,config.policy().unwrap(),character_preferences(&config,&character,minute).unwrap());
            chooser.choose(0,&state,&offers,None).unwrap().unwrap().offer().need()
        };
        assert_eq!(choose(routine_minute(at,&local).unwrap()),Need::Engagement);
        assert_eq!(choose(routine_minute(at,&chrono::Utc).unwrap()),Need::Energy);
    }

    #[test]
    fn observed_goal_options_persist_pause_and_resume_without_starting_life() {
        use crate::character_development::GoalStatus;
        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture { fn drop(&mut self) {let _=std::fs::remove_dir_all(&self.0);} }
        let fixture=Fixture(std::env::temp_dir().join(format!("orbitpal-care-goal-{}",uuid::Uuid::new_v4())));
        let store=crate::storage::Store::new(fixture.0.clone()).unwrap();
        let bot=store.profile().unwrap()["id"].as_str().unwrap().to_owned();
        store.social_reconcile_goal(&bot,&goal_options(&[],false,false)).unwrap();
        assert!(store.social_snapshot(&bot).unwrap().development.goal.is_none());
        // A supported real Here offer is sufficient for a small Play goal;
        // it claims neither a station lease nor a completed activity.
        let offers=vec![Offer::new(Need::Engagement,Target::Here,1000).unwrap()];
        store.social_reconcile_goal(&bot,&goal_options(&offers,false,false)).unwrap();
        let initial=store.social_snapshot(&bot).unwrap();
        let goal=initial.development.goal.clone().unwrap();
        assert_eq!(goal.kind,HobbyKind::Play);assert_eq!(goal.progress,0);
        store.social_reconcile_goal(&bot,&goal_options(&offers,false,false)).unwrap();
        assert_eq!(store.social_snapshot(&bot).unwrap().revision,initial.revision,"unchanged availability must not churn revisions");
        store.social_reconcile_goal(&bot,&goal_options(&[],false,false)).unwrap();
        assert_eq!(store.social_snapshot(&bot).unwrap().development.goal.unwrap().status,GoalStatus::Paused);
        drop(store);
        let store=crate::storage::Store::new(fixture.0.clone()).unwrap();
        assert_eq!(store.social_snapshot(&bot).unwrap().development.goal.unwrap().status,GoalStatus::Paused);
        store.social_reconcile_goal(&bot,&goal_options(&offers,false,false)).unwrap();
        let resumed=store.social_snapshot(&bot).unwrap().development.goal.unwrap();
        assert_eq!(resumed,goal);
        assert!(store.load_life(&bot).unwrap().is_none(),"a persisted goal cannot start a Life session");
    }

    #[test]
    fn company_and_exploration_require_their_distinct_current_observations() {
        use crate::character_development::Character;
        let mut character=Character::default();
        character.interests.company=1000;character.interests.explore=800;character.interests.play=0;
        character.reconcile_goal(&goal_options(&[],false,false));
        assert!(character.goal.is_none());
        character.reconcile_goal(&goal_options(&[],true,false));
        assert_eq!(character.goal.as_ref().unwrap().kind,HobbyKind::Explore);
        character.reconcile_goal(&goal_options(&[],false,true));
        assert_eq!(character.goal.as_ref().unwrap().status,crate::character_development::GoalStatus::Paused);
        character.goal=None;
        character.reconcile_goal(&goal_options(&[],false,true));
        assert_eq!(character.goal.as_ref().unwrap().kind,HobbyKind::Company);
        assert_eq!(character.goal.as_ref().unwrap().progress,0);
    }
}
