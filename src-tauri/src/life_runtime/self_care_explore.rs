//! Advisory Explore selection over actual free World slots. Nothing here can
//! reserve a slot, start/finish an action, grant a clip or change earned state.
use crate::{
    character_development::{Character, GoalStatus, HobbyKind},
    experience::Preferences,
    life_domain::{Needs, MAX_COUNTER},
    life_self_care::{CareConfig, End},
    world_hub::ActionSlotTarget,
    world_output::{CssRect, CssSize, MotionBinding},
    world_state::{coordinates::Point, SlotId},
};

/// A copy of measured geometry/capabilities, never a NativeActionPermit.
#[derive(Clone, Copy)]
pub(super) struct WalkObservation {
    viewport: CssSize,
    safe: CssRect,
    supported: bool,
}
impl WalkObservation {
    pub(super) fn from_binding(binding: &MotionBinding) -> Self {
        Self { viewport: binding.viewport, safe: binding.safe_centers,
            supported: binding.walk_supported && !binding.reduced_motion }
    }
}

pub(super) fn candidates<'a>(
    slots: &'a [ActionSlotTarget], home: Point, position: Point,
    preferences: &Preferences, observation: WalkObservation,
) -> Vec<&'a ActionSlotTarget> {
    if !observation.supported || !observation.viewport.valid() || !observation.safe.valid()
        || !["idle", "walk"].iter().all(|motion|
            preferences.allowed_animations.iter().any(|allowed|allowed.as_str()==*motion)) {
        return Vec::new();
    }
    let from=crate::world_output::css(position, observation.viewport);
    if !observation.safe.contains(from) {return Vec::new();}
    let mut result=slots.iter().filter(|slot| {
        let target=crate::world_output::css(slot.position, observation.viewport);
        // A subpixel/no-op arrival is not a useful exploration opportunity.
        slot.position!=position && from.distance(target)>=12.0
            && super::within_radius(home,slot.position,preferences.movement_radius)
            && observation.safe.contains(target)
    }).collect::<Vec<_>>();
    result.sort_by(|a,b| {
        let distance=|slot: &ActionSlotTarget|
            u64::from(slot.position.x().abs_diff(position.x()))
                + u64::from(slot.position.y().abs_diff(position.y()));
        distance(a).cmp(&distance(b)).then(a.id.cmp(&b.id))
    });
    result
}

#[derive(Default)]
pub(super) struct Memory {
    next: u64,
    blocked: Option<(SlotId,u64)>,
}
impl Memory {
    pub(super) fn choose<'a>(
        &self, now: u64, character: &Character, needs: Needs, config: &CareConfig,
        targets: &[&'a ActionSlotTarget],
    ) -> Option<&'a ActionSlotTarget> {
        if now<self.next || now>MAX_COUNTER
            || [needs.energy().units(),needs.nourishment().units(),needs.engagement().units(),needs.social().units()]
                .iter().zip(config.start_below).any(|(level,threshold)|*level<threshold)
            || !character.goal.as_ref().is_some_and(|goal|
                goal.kind==HobbyKind::Explore && goal.status==GoalStatus::Active) {
            return None;
        }
        let targets=targets.iter().copied().filter(|slot|
            !self.blocked.as_ref().is_some_and(|(id,until)|id==&slot.id && now<*until))
            .collect::<Vec<_>>();
        if targets.is_empty() {return None;}
        // Earned Explore variation broadens the existing supported Walk's real
        // destination choices. It never invents an embedded gesture/animation.
        let (variation,experience)=character.hobbies.iter().find(|hobby|hobby.kind==HobbyKind::Explore)
            .map_or((0,0),|hobby|(hobby.variation.min(3),hobby.experience));
        let breadth=(usize::from(variation)+1).min(targets.len());
        let index=usize::from(experience/25)%breadth;
        targets.get(index).copied()
    }
    pub(super) fn record_end(&mut self, now: u64, slot: &SlotId, end: End, config: &CareConfig, initiative_cooldown_ms: u64) {
        let delay=if end==End::Finished {config.cooldown_ms} else {config.backoff_ms};
        self.next=now.saturating_add(delay.max(initiative_cooldown_ms)).min(MAX_COUNTER);
        if end!=End::Finished {self.blocked=Some((slot.clone(),self.next));}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{character_development::Goal,world_state::AnchorKind};
    fn point(x:u32)->Point {Point::new(x,500_000).unwrap()}
    fn slot(id:&str,x:u32)->ActionSlotTarget {ActionSlotTarget{anchor_id:crate::world_state::AnchorId::new("fixture-object").unwrap(),id:SlotId::new(id).unwrap(),kind:AnchorKind::Play,position:point(x),content_ref:None}}
    fn observation()->WalkObservation {WalkObservation{viewport:CssSize{width:1000.,height:1000.},safe:CssRect{left:100.,top:100.,right:900.,bottom:900.},supported:true}}
    fn preferences()->Preferences {Preferences{movement_radius:400,..Preferences::default()}}
    fn character()->Character {let mut value=Character::default();value.goal=Some(Goal{id:"explore".into(),kind:HobbyKind::Explore,target:5,progress:0,status:GoalStatus::Active});value}
    fn healthy()->Needs {Needs::new(9000,9000,9000,9000).unwrap()}
    #[test]
    fn only_real_reachable_free_slots_survive_measured_geometry_and_permissions() {
        let slots=vec![slot("same",500_000),slot("tiny",501_000),slot("outside",950_000),slot("near",650_000),slot("far",800_000)];
        let prefs=preferences();
        let targets=candidates(&slots,point(500_000),point(500_000),&prefs,observation());
        assert_eq!(targets.iter().map(|slot|slot.id.as_str()).collect::<Vec<_>>(),vec!["near","far"]);
        let mut disabled=prefs.clone();disabled.allowed_animations.retain(|motion|motion!="walk");
        assert!(candidates(&slots,point(500_000),point(500_000),&disabled,observation()).is_empty());
        disabled=prefs.clone();disabled.allowed_animations.retain(|motion|motion!="idle");
        assert!(candidates(&slots,point(500_000),point(500_000),&disabled,observation()).is_empty());
        disabled=prefs;disabled.movement_radius=0;
        assert!(candidates(&slots,point(500_000),point(500_000),&disabled,observation()).is_empty());
        let mut unavailable=observation();unavailable.supported=false;
        assert!(candidates(&slots,point(500_000),point(500_000),&preferences(),unavailable).is_empty());
        unavailable=observation();unavailable.viewport.width=f64::NAN;
        assert!(candidates(&slots,point(500_000),point(500_000),&preferences(),unavailable).is_empty());
        assert!(candidates(&[],point(500_000),point(500_000),&preferences(),observation()).is_empty());
    }
    #[test]
    fn every_urgent_need_blocks_exploration_even_if_care_has_no_available_offer() {
        let slots=vec![slot("near",650_000)];let targets=vec![&slots[0]];
        let memory=Memory::default();let character=character();let config=CareConfig::default();
        for index in 0..4 {let mut levels=[9000;4];levels[index]=config.start_below[index]-1;
            let needs=Needs::new(levels[0],levels[1],levels[2],levels[3]).unwrap();
            assert!(memory.choose(0,&character,needs,&config,&targets).is_none());}
        assert!(memory.choose(0,&character,healthy(),&config,&targets).is_some());
        let mut inactive=character;inactive.goal.as_mut().unwrap().status=GoalStatus::Paused;
        assert!(memory.choose(0,&inactive,healthy(),&config,&targets).is_none());
    }
    #[test]
    fn actual_terminal_observations_enforce_cooldown_and_failed_target_backoff() {
        let slots=vec![slot("near",650_000)];let targets=vec![&slots[0]];
        let mut memory=Memory::default();let character=character();let config=CareConfig::default();
        memory.record_end(1000,&slots[0].id,End::Finished,&config,5000);
        assert!(memory.choose(5999,&character,healthy(),&config,&targets).is_none());
        assert!(memory.choose(6000,&character,healthy(),&config,&targets).is_some());
        memory.record_end(6000,&slots[0].id,End::Interrupted,&config,5000);
        assert!(memory.choose(20_999,&character,healthy(),&config,&targets).is_none());
        assert!(memory.choose(21_000,&character,healthy(),&config,&targets).is_some());
        assert_eq!(character.goal.as_ref().unwrap().progress,0,"selector/end observations cannot issue rewards");
    }
    #[test]
    fn earned_variation_selects_another_existing_walk_destination_without_mutating_progress() {
        let slots=vec![slot("one",600_000),slot("two",650_000),slot("three",700_000)];
        let targets=candidates(&slots,point(500_000),point(500_000),&preferences(),observation());
        let memory=Memory::default();let mut learned=character();let config=CareConfig::default();
        assert_eq!(memory.choose(0,&learned,healthy(),&config,&targets).unwrap().id.as_str(),"one");
        let hobby=learned.hobbies.iter_mut().find(|hobby|hobby.kind==HobbyKind::Explore).unwrap();
        hobby.experience=275;hobby.variation=1;
        let before=learned.clone();
        assert_eq!(memory.choose(0,&learned,healthy(),&config,&targets).unwrap().id.as_str(),"two");
        assert_eq!(learned,before);
        assert!(memory.choose(0,&learned,healthy(),&config,&[]).is_none());
    }
}
