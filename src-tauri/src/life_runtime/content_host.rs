//! Exact active declarative templates can tune bounded existing actions only.
use crate::{
    content_packs::{self, Action, Condition, ContentRef, Installed, MotionCapability},
    life_domain::{Activity, LifeState},
    world_state::AnchorKind,
};
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContentUse {
    pub reference: ContentRef,
    pub catalog_revision: u64,
    pub behavior: Option<AppliedBehavior>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppliedBehavior {
    pub id: String,
    pub duration_ms: u64,
    pub weight: u16,
    pub condition: Condition,
}
/// All supported frozen definitions, ordered independently of mutable needs.
fn definitions(
    packs: &[Installed],
    reference: &ContentRef,
    kind: AnchorKind,
    activity: Activity,
) -> Option<Vec<ContentUse>> {
    let object = content_packs::resolve_object(packs, reference, kind)?;
    let pack = packs.iter().find(|p| {
        p.active
            && p.manifest.id == reference.package_id
            && p.manifest.version == reference.package_version
    })?;
    if activity == Activity::MoveTo {
        return Some(vec![ContentUse {
            reference: reference.clone(),
            catalog_revision: pack.revision,
            behavior: None,
        }]);
    }
    let action = match activity {
        Activity::Rest => Action::Rest,
        Activity::Snack => Action::Snack,
        Activity::Play => Action::Play,
        Activity::Socialize => Action::Socialize,
        _ => return None,
    };
    let manifest = &pack.manifest;
    let mut choices = manifest
        .activities
        .iter()
        .filter(|behavior| {
            behavior.action == action
                && behavior.object_id == object.id
                && object.offers.contains(&behavior.id)
        })
        .filter(|behavior| {
            manifest
                .motions
                .iter()
                .find(|motion| motion.id == behavior.motion_id)
                .is_some_and(|motion| {
                    motion.capability == MotionCapability::Idle
                        || motion.fallback == MotionCapability::Idle
                })
        })
        .collect::<Vec<_>>();
    // The current station adapter presents the measured idle pose. Other
    // motions require an explicitly declared idle fallback; no clip is invented.
    choices.sort_by(|a, b| b.weight.cmp(&a.weight).then(a.id.cmp(&b.id)));
    Some(
        choices
            .into_iter()
            .map(|behavior| ContentUse {
                reference: reference.clone(),
                catalog_revision: pack.revision,
                behavior: Some(AppliedBehavior {
                    id: behavior.id.clone(),
                    duration_ms: u64::from(behavior.duration_ms),
                    weight: behavior.weight,
                    condition: behavior.condition,
                }),
            })
            .collect(),
    )
}
pub(crate) fn resolve(
    packs: &[Installed],
    reference: &ContentRef,
    kind: AnchorKind,
    activity: Activity,
) -> Option<ContentUse> {
    definitions(packs, reference, kind, activity)?
        .into_iter()
        .next()
}
/// Conditions filter candidates BEFORE weight selection; a false high-weight
/// offer cannot hide an eligible alternative on the same real object.
pub(crate) fn resolve_for_state(
    packs: &[Installed],
    reference: &ContentRef,
    kind: AnchorKind,
    activity: Activity,
    state: &LifeState,
    config: &crate::life_self_care::CareConfig,
) -> Option<ContentUse> {
    definitions(packs, reference, kind, activity)?
        .into_iter()
        .find(|usage| condition(usage, state, config))
}

/// Revalidate the chosen ID and its exact frozen definition/revision, never
/// reselect using needs changed by progress or the eventual genuine reward.
pub(crate) fn definition_current(
    packs: &[Installed],
    usage: &ContentUse,
    kind: AnchorKind,
    activity: Activity,
) -> bool {
    definitions(packs, &usage.reference, kind, activity)
        .is_some_and(|definitions| definitions.iter().any(|actual| actual == usage))
}
pub(crate) fn condition(
    usage: &ContentUse,
    state: &LifeState,
    config: &crate::life_self_care::CareConfig,
) -> bool {
    let Some(behavior) = &usage.behavior else {
        return true;
    };
    let needs = state.needs();
    let (index, level) = match behavior.condition {
        Condition::LowEnergy => (0, needs.energy().units()),
        Condition::LowNourishment => (1, needs.nourishment().units()),
        Condition::LowEngagement => (2, needs.engagement().units()),
        Condition::LowSocial => (3, needs.social().units()),
    };
    level < config.start_below[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packs() -> Vec<Installed> {
        vec![Installed {
            manifest: crate::content_packs::Manifest::parse(include_str!(
                "../../../public/content-packs/desk-garden.json"
            ))
            .unwrap(),
            active: true,
            revision: 2,
        }]
    }
    #[test]
    fn exact_active_objects_bind_real_behavior_and_explicit_idle_fallback() {
        let mut packs = packs();
        let object = packs[0]
            .manifest
            .objects
            .iter()
            .find(|object| object.kind == content_packs::ObjectKind::Play)
            .unwrap();
        let reference = ContentRef {
            package_id: "desk-garden".into(),
            package_version: 1,
            template_id: object.id.clone(),
        };
        let usage = resolve(&packs, &reference, AnchorKind::Play, Activity::Play).unwrap();
        assert!(usage.behavior.as_ref().unwrap().duration_ms > 0);
        assert!(resolve(&packs, &reference, AnchorKind::Snack, Activity::Snack).is_none());
        let mut newer = reference.clone();
        newer.package_version = 2;
        assert!(resolve(&packs, &newer, AnchorKind::Play, Activity::Play).is_none());
        packs[0].active = false;
        assert!(!definition_current(
            &packs,
            &usage,
            AnchorKind::Play,
            Activity::Play
        ));
        packs[0].active = true;
        for motion in &mut packs[0].manifest.motions {
            motion.capability = MotionCapability::Wave;
            motion.fallback = MotionCapability::Walk;
        }
        assert!(resolve(&packs, &reference, AnchorKind::Play, Activity::Play).is_none());
        assert!(
            resolve(&packs, &reference, AnchorKind::Play, Activity::MoveTo)
                .unwrap()
                .behavior
                .is_none()
        );
    }
    #[test]
    fn conditions_read_confirmed_needs_without_changing_them() {
        use crate::life_domain::{ActorId, Needs, WorldId};
        let packs = packs();
        let object = packs[0]
            .manifest
            .objects
            .iter()
            .find(|object| object.kind == content_packs::ObjectKind::Play)
            .unwrap();
        let reference = ContentRef {
            package_id: "desk-garden".into(),
            package_version: 1,
            template_id: object.id.clone(),
        };
        let usage = resolve(&packs, &reference, AnchorKind::Play, Activity::Play).unwrap();
        let state = LifeState::new(
            ActorId::new("nova").unwrap(),
            WorldId::new("world_nova").unwrap(),
            Needs::new(9000, 9000, 9000, 9000).unwrap(),
        );
        assert!(!condition(&usage, &state, &Default::default()));
        assert!(state.current_action().is_none());
    }
    #[test]
    fn an_ineligible_high_weight_offer_cannot_hide_an_eligible_play_alternative() {
        use crate::life_domain::{ActorId, Needs, WorldId};
        let mut packs = packs();
        let mut high = packs[0].manifest.activities[0].clone();
        high.id = "desk-garden:energy-play".into();
        high.condition = Condition::LowEnergy;
        high.weight = 900;
        high.duration_ms = 12_000;
        packs[0].manifest.objects[0].offers.push(high.id.clone());
        packs[0].manifest.activities.push(high);
        packs[0].manifest.validate().unwrap();
        let reference = ContentRef {
            package_id: "desk-garden".into(),
            package_version: 1,
            template_id: "desk-garden:seed".into(),
        };
        let state = LifeState::new(
            ActorId::new("nova").unwrap(),
            WorldId::new("world_nova").unwrap(),
            Needs::new(9000, 9000, 1000, 9000).unwrap(),
        );
        let config = crate::life_self_care::CareConfig::default();
        let chosen = resolve_for_state(
            &packs,
            &reference,
            AnchorKind::Play,
            Activity::Play,
            &state,
            &config,
        )
        .unwrap();
        let behavior = chosen.behavior.as_ref().unwrap();
        assert_eq!(behavior.id, "desk-garden:tend");
        assert_eq!(behavior.weight, 400);
        assert_eq!(behavior.duration_ms, 4000);
        assert!(condition(&chosen, &state, &config));
        assert!(definition_current(
            &packs,
            &chosen,
            AnchorKind::Play,
            Activity::Play
        ));
        // An admission recheck still rejects a now-ineligible chosen offer.
        // Later definition checks retain its identity rather than selecting a
        // different offer after actual needs change. This creates no Complete.
        let changed = LifeState::new(
            ActorId::new("nova").unwrap(),
            WorldId::new("world_nova").unwrap(),
            Needs::new(1000, 9000, 9000, 9000).unwrap(),
        );
        assert!(!condition(&chosen, &changed, &config));
        assert_eq!(
            resolve_for_state(
                &packs,
                &reference,
                AnchorKind::Play,
                Activity::Play,
                &changed,
                &config
            )
            .unwrap()
            .behavior
            .unwrap()
            .id,
            "desk-garden:energy-play"
        );
        assert!(definition_current(
            &packs,
            &chosen,
            AnchorKind::Play,
            Activity::Play
        ));
        assert!(state.current_action().is_none());
        assert!(changed.current_action().is_none());
        let mut modified = packs.clone();
        modified[0].manifest.activities[0].duration_ms += 1;
        assert!(!definition_current(
            &modified,
            &chosen,
            AnchorKind::Play,
            Activity::Play
        ));
        modified = packs.clone();
        modified[0].revision += 1;
        assert!(!definition_current(
            &modified,
            &chosen,
            AnchorKind::Play,
            Activity::Play
        ));
        modified = packs;
        modified[0].active = false;
        assert!(resolve_for_state(
            &modified,
            &reference,
            AnchorKind::Play,
            Activity::Play,
            &state,
            &config
        )
        .is_none());
    }
}
