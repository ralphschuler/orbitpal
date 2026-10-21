//! Read-only menu offers over the actual native owner. These booleans never
//! reserve a slot, mint a permit or replace selection-time action admission.
use crate::{
    experience::{ExperienceRuntime, Preferences},
    world_hub::ActionSlotTarget,
    world_output::{CssPoint, CssRect, CssSize},
    world_state::{coordinates::Point, AnchorKind},
    AppState,
};
use tauri::Manager;

const ERROR: &str = "interaction_unavailable";

#[derive(Default)]
struct Choices {
    active_owner: bool,
    paused: bool,
    busy: bool,
    output_allowed: bool,
    pet: bool,
    play: bool,
    home: bool,
}
impl Choices {
    fn enabled(&self) -> [bool; 4] {
        if !self.active_owner || self.paused {
            return [false; 4];
        }
        let free = !self.busy && self.output_allowed;
        [free && self.pet, free && self.play, free && self.home, true]
    }
}

fn allowed(preferences: &Preferences, animation: &str) -> bool {
    preferences
        .allowed_animations
        .iter()
        .any(|item| item == animation)
}

/// The exact authored Pet clip and its duration have already passed the
/// native asset/configuration catalogue. Selection still performs admission.
fn pet_available(preferences: &Preferences, clip: Option<&crate::world_output::PetClip>) -> bool {
    allowed(preferences, "pet") && clip.is_some_and(|clip|clip.valid())
}

/// Match the executor wrapper's actual nearest-slot choice. Do not advertise a
/// farther fitting slot when the current handler would choose a nearer one.
fn selected_slot(
    slots: &[ActionSlotTarget],
    kind: AnchorKind,
    from: Point,
) -> Option<&ActionSlotTarget> {
    slots
        .iter()
        .filter(|slot| slot.kind == kind)
        .min_by(|a, b| {
            let cost = |slot: &ActionSlotTarget| {
                u64::from(slot.position.x().abs_diff(from.x()))
                    + u64::from(slot.position.y().abs_diff(from.y()))
            };
            cost(a).cmp(&cost(b)).then(a.id.cmp(&b.id))
        })
}

struct Movement {
    position: Point,
    home: Point,
    from: CssPoint,
    viewport: CssSize,
    safe: CssRect,
    walk_supported: bool,
    reduced_motion: bool,
}
impl Movement {
    fn allows(&self, preferences: &Preferences, target: Point) -> bool {
        allowed(preferences, "idle")
            && self.safe.contains(self.from)
            && self
                .safe
                .contains(crate::world_output::css(target, self.viewport))
            && super::self_care::within_radius(self.home, target, preferences.movement_radius)
            && (target == self.position
                || (allowed(preferences, "walk") && self.walk_supported && !self.reduced_motion))
    }
}

fn home_available(preferences: &Preferences, movement: &Movement, slots: &[ActionSlotTarget]) -> bool {
    selected_slot(slots, AnchorKind::Rest, movement.position)
        .is_some_and(|slot| movement.allows(preferences, slot.position))
}

/// Pet/Play/Home/Pause in native menu order. Failure supplies no optimistic
/// capability. The caller must still revalidate its retained MenuOwner.
pub(crate) fn native_interaction_choices(app: &tauri::AppHandle, bot: &str) -> [bool; 4] {
    choices(app, bot).map_or([false; 4], |choices| choices.enabled())
}

fn choices(app: &tauri::AppHandle, bot: &str) -> Result<Choices, String> {
    let state = app.state::<AppState>();
    let runtime = &state.life_runtime;
    state.updater.ensure_idle()?;
    if runtime.stopping.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(ERROR.into());
    }
    // Care's chooser owns its lock before Owned/Core. Asset inspection also
    // stays outside these locks; imported model/motion files may require I/O.
    let origin = runtime.care.origin(bot);
    let profile = state.store.bot_profile(bot)?;
    let pet_clip = crate::emote_capabilities::pet_clip(&state.store.root,&profile).ok();
    let external_busy = runtime.social.busy(bot)
        || runtime.desktop.busy(bot)
        || state.overlay.has_any_pending_speech()
        || state.generation.try_lock().is_err();
    let members = runtime.native_scenes.members(Some(bot));
    let [member] = members.as_slice() else {
        return Err(ERROR.into());
    };
    let owned_guard = member.owned.lock().map_err(|_| ERROR)?;
    let owned = owned_guard.as_ref().ok_or(ERROR)?;
    let core = runtime.core.lock().map_err(|_| ERROR)?;
    let now = runtime.now();
    if !member.current()
        || !runtime
            .session_outputs
            .constructor_current(&member.constructor)
        || owned.context.profile != profile
        || !owned.context.current(&core, &state, now)
    {
        return Err(ERROR.into());
    }
    let output = core.world_outputs.ready(bot, now).map_err(|_| ERROR)?;
    let permit = owned.action_permit(now).ok_or(ERROR)?;
    if !permit.current(&core, now)
        || !permit.matches_output(&output)
        || !core.world_outputs.check_proof(&output, now)
    {
        return Err(ERROR.into());
    }
    let session = core.sessions.get(bot).ok_or(ERROR)?;
    let preferences = state.store.experience_settings(bot)?.preferences;
    let mut result = Choices {
        active_owner: session.mode == super::core::Mode::Life
            && session.output
            && !session.storage_failed
            && session.state.is_some(),
        paused: app.state::<ExperienceRuntime>().paused(bot)?,
        busy: external_busy
            || core.local_action_reason(bot, now).is_some()
            || core.actions.as_ref().is_none_or(|actions| {
                session
                    .state
                    .as_ref()
                    .is_none_or(|life| !actions.executor.can_admit(life.actor_id()))
            }),
        output_allowed: runtime.outputs.ticket().is_some(),
        pet: pet_available(&preferences, pet_clip.as_ref())
            && output.binding.station_supported && !output.binding.reduced_motion,
        ..Default::default()
    };
    if !result.active_owner || result.paused || result.busy || !result.output_allowed {
        return Ok(result);
    }
    // Owned -> Core -> World -> Store, never another member's Owned lock.
    let source = owned
        .context
        .source(&core, &state, owned.route.clone(), owned.framing, now)?;
    if !core.world_outputs.matches(bot, &source) {
        return Err(ERROR.into());
    }
    let home = origin
        .filter(|(session_id, context, _)| {
            session_id == &session.session_id
                && context
                    == &(
                        source.context.instance_id.clone(),
                        source.context.config_revision,
                        source.context.topology_revision,
                    )
        })
        .map_or(source.context.position, |(_, _, home)| home);
    let movement = Movement {
        position: source.context.position,
        home,
        from: output.binding.position,
        viewport: output.binding.viewport,
        safe: output.binding.safe_centers,
        walk_supported: output.binding.walk_supported,
        reduced_motion: output.binding.reduced_motion,
    };
    // action_slots returns only free current slots and rejects inactive/missing
    // exact content references. No observation reserves them for this menu.
    let slots = state.world.action_slots(&state.store, &source.context)?;
    result.play = allowed(&preferences, "play") && allowed(&preferences, "idle")
        && output.binding.station_supported;
    result.home = home_available(&preferences, &movement, &slots);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_state::SlotId;

    fn point(x: u32, y: u32) -> Point {
        Point::new(x, y).unwrap()
    }
    fn movement() -> Movement {
        Movement {
            position: point(500_000, 500_000),
            home: point(500_000, 500_000),
            from: CssPoint { x: 500., y: 400. },
            viewport: CssSize {
                width: 1000.,
                height: 800.,
            },
            safe: CssRect {
                left: 100.,
                top: 80.,
                right: 900.,
                bottom: 720.,
            },
            walk_supported: true,
            reduced_motion: false,
        }
    }
    #[test]
    fn inactive_paused_busy_and_revoked_output_have_distinct_offers() {
        let mut choices = Choices {
            active_owner: true,
            output_allowed: true,
            pet: true,
            play: true,
            home: true,
            ..Default::default()
        };
        assert_eq!(choices.enabled(), [true; 4]);
        choices.busy = true;
        assert_eq!(choices.enabled(), [false, false, false, true]);
        choices.busy = false;
        choices.output_allowed = false;
        assert_eq!(choices.enabled(), [false, false, false, true]);
        choices.output_allowed = true;
        choices.paused = true;
        assert_eq!(choices.enabled(), [false; 4]);
        choices.paused = false;
        choices.active_owner = false;
        assert_eq!(choices.enabled(), [false; 4]);
    }
    #[test]
    fn pet_needs_explicit_preference_and_exact_bounded_authored_clip() {
        let mut prefs=Preferences::default();
        let clip=crate::world_output::PetClip{action:"Pet".into(),duration_ms:1200};
        assert!(!pet_available(&prefs,Some(&clip)));
        prefs.allowed_animations.push("pet".into());
        assert!(pet_available(&prefs,Some(&clip)));
        assert!(!pet_available(&prefs,None));
        assert!(!pet_available(&prefs,Some(&crate::world_output::PetClip{action:"Wave".into(),duration_ms:1200})));
        assert!(!pet_available(&prefs,Some(&crate::world_output::PetClip{action:"Pet".into(),duration_ms:15_001})));
    }
    #[test]
    fn real_slot_selection_is_kind_filtered_nearest_and_stable_on_ties() {
        let slot = |id, kind, x| ActionSlotTarget {
            id: SlotId::new(id).unwrap(),
            kind,
            position: point(x, 500_000),
            content_ref: None,
        };
        let slots = vec![
            slot("closer-play", AnchorKind::Play, 500_000),
            slot("rest-b", AnchorKind::Rest, 550_000),
            slot("rest-a", AnchorKind::Rest, 450_000),
            slot("far-rest", AnchorKind::Rest, 750_000),
        ];
        assert_eq!(
            selected_slot(&slots, AnchorKind::Rest, point(500_000, 500_000))
                .unwrap()
                .id
                .as_str(),
            "rest-a"
        );
        assert!(selected_slot(&[], AnchorKind::Rest, point(500_000, 500_000)).is_none());
        assert!(selected_slot(&slots, AnchorKind::Snack, point(500_000, 500_000)).is_none());
    }
    #[test]
    fn home_requires_a_free_rest_slot_not_a_play_slot_or_here_fallback() {
        let slots = vec![ActionSlotTarget {
            id: SlotId::new("play-seat").unwrap(),
            kind: AnchorKind::Play,
            position: point(500_000, 500_000),
            content_ref: None,
        }];
        let prefs = Preferences::default();
        let geometry = movement();
        assert!(!home_available(&prefs, &geometry, &slots));
        assert!(!home_available(&prefs, &geometry, &[]));
    }
    #[test]
    fn home_needs_idle_permission_but_not_station_completion_support() {
        let geometry = movement();
        let mut prefs = Preferences::default();
        let slots = vec![ActionSlotTarget {
            id: SlotId::new("rest-seat").unwrap(), kind: AnchorKind::Rest,
            position: geometry.position, content_ref: None,
        }];
        assert!(home_available(&prefs, &geometry, &slots));
        prefs.allowed_animations.retain(|name| name != "idle");
        assert!(!home_available(&prefs, &geometry, &slots));
    }
    #[test]
    fn an_invalid_actual_nearest_target_is_not_replaced_by_a_different_menu_target() {
        let mut geometry = movement();
        geometry.home = point(200_000, 500_000);
        let prefs = Preferences {
            movement_radius: 100,
            ..Default::default()
        };
        let slots = vec![
            ActionSlotTarget {
                id: SlotId::new("nearest-rest").unwrap(),
                kind: AnchorKind::Rest,
                position: point(450_000, 500_000),
                content_ref: None,
            },
            ActionSlotTarget {
                id: SlotId::new("fitting-rest").unwrap(),
                kind: AnchorKind::Rest,
                position: point(250_000, 500_000),
                content_ref: None,
            },
        ];
        assert!(geometry.allows(&prefs, slots[1].position));
        assert!(!home_available(&prefs, &geometry, &slots));
    }
    #[test]
    fn safe_centers_use_actual_viewport_coordinates_and_current_origin() {
        let prefs = Preferences {
            movement_radius: 1000,
            ..Default::default()
        };
        let mut geometry = movement();
        assert!(geometry.allows(&prefs, point(900_000, 500_000)));
        assert!(!geometry.allows(&prefs, point(900_001, 500_000)));
        geometry.from.x = 99.;
        assert!(!geometry.allows(&prefs, geometry.position));
    }
    #[test]
    fn movement_needs_current_walk_capability_preference_and_radius() {
        let target = point(600_000, 500_000);
        let mut geometry = movement();
        let mut prefs = Preferences::default();
        assert!(geometry.allows(&prefs, target));
        prefs.movement_radius = 0;
        assert!(!geometry.allows(&prefs, target));
        assert!(geometry.allows(&prefs, geometry.position));
        prefs.movement_radius = 250;
        prefs.allowed_animations.retain(|name| name != "walk");
        assert!(!geometry.allows(&prefs, target));
        assert!(geometry.allows(&prefs, geometry.position));
        prefs.allowed_animations.push("walk".into());
        geometry.walk_supported = false;
        assert!(!geometry.allows(&prefs, target));
        geometry.walk_supported = true;
        geometry.reduced_motion = true;
        assert!(!geometry.allows(&prefs, target));
        geometry.reduced_motion = false;
        prefs.allowed_animations.retain(|name| name != "idle");
        assert!(!geometry.allows(&prefs, geometry.position));
    }
}
