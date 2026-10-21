//! Exact active declarative templates can tune bounded existing actions only.
use crate::{content_packs::{self, ContentRef, Installed, Action, Condition, MotionCapability}, life_domain::{Activity, LifeState}, world_state::AnchorKind};
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
pub(crate) fn resolve(packs:&[Installed], reference:&ContentRef, kind:AnchorKind, activity:Activity)->Option<ContentUse> {
    let object=content_packs::resolve_object(packs,reference,kind)?;
    let pack=packs.iter().find(|p|p.active&&p.manifest.id==reference.package_id&&p.manifest.version==reference.package_version)?;
    if activity==Activity::MoveTo {return Some(ContentUse{reference:reference.clone(),catalog_revision:pack.revision,behavior:None});}
    let action=match activity {Activity::Rest=>Action::Rest,Activity::Snack=>Action::Snack,
        Activity::Play=>Action::Play,Activity::Socialize=>Action::Socialize,_=>return None};
    let manifest=&pack.manifest;
    let mut choices=manifest.activities.iter().filter(|behavior|behavior.action==action&&behavior.object_id==object.id&&object.offers.contains(&behavior.id))
        .filter(|behavior|manifest.motions.iter().find(|motion|motion.id==behavior.motion_id).is_some_and(|motion|
            motion.capability==MotionCapability::Idle||motion.fallback==MotionCapability::Idle)).collect::<Vec<_>>();
    // The current station adapter presents the measured idle pose. Other
    // motions require an explicitly declared idle fallback; no clip is invented.
    choices.sort_by(|a,b|b.weight.cmp(&a.weight).then(a.id.cmp(&b.id)));
    let behavior=choices.first()?;
    Some(ContentUse{reference:reference.clone(),catalog_revision:pack.revision,behavior:Some(AppliedBehavior{id:behavior.id.clone(),duration_ms:u64::from(behavior.duration_ms),weight:behavior.weight,condition:behavior.condition})})
}
pub(crate) fn kind(activity:Activity)->Option<AnchorKind> {Some(match activity {Activity::Rest=>AnchorKind::Rest,Activity::Snack=>AnchorKind::Snack,Activity::Play=>AnchorKind::Play,Activity::Socialize=>AnchorKind::Meeting,_=>return None})}
pub(crate) fn definition_current(packs:&[Installed], usage:&ContentUse, kind:AnchorKind, activity:Activity)->bool {resolve(packs,&usage.reference,kind,activity).as_ref()==Some(usage)}
pub(crate) fn condition(usage:&ContentUse, state:&LifeState, config:&crate::life_self_care::CareConfig)->bool {
    let Some(behavior)=&usage.behavior else{return true;};
    let needs=state.needs(); let (index,level)=match behavior.condition {
        Condition::LowEnergy=>(0,needs.energy().units()),Condition::LowNourishment=>(1,needs.nourishment().units()),
        Condition::LowEngagement=>(2,needs.engagement().units()),Condition::LowSocial=>(3,needs.social().units())};
    level<config.start_below[index]
}

#[cfg(test)] mod tests {
    use super::*;
    fn packs()->Vec<Installed> {vec![Installed{manifest:crate::content_packs::Manifest::parse(include_str!("../../../public/content-packs/desk-garden.json")).unwrap(),active:true,revision:2}]}
    #[test] fn exact_active_objects_bind_real_behavior_and_explicit_idle_fallback() {
        let mut packs=packs(); let object=packs[0].manifest.objects.iter().find(|object|object.kind==content_packs::ObjectKind::Rest).unwrap();
        let reference=ContentRef{package_id:"desk-garden".into(),package_version:1,template_id:object.id.clone()};
        let usage=resolve(&packs,&reference,AnchorKind::Rest,Activity::Rest).unwrap();
        assert!(usage.behavior.as_ref().unwrap().duration_ms>0);
        assert!(resolve(&packs,&reference,AnchorKind::Snack,Activity::Snack).is_none());
        let mut newer=reference.clone();newer.package_version=2;assert!(resolve(&packs,&newer,AnchorKind::Rest,Activity::Rest).is_none());
        packs[0].active=false;assert!(!definition_current(&packs,&usage,AnchorKind::Rest,Activity::Rest));
        packs[0].active=true;
        for motion in &mut packs[0].manifest.motions {motion.capability=MotionCapability::Wave;motion.fallback=MotionCapability::Walk;}
        assert!(resolve(&packs,&reference,AnchorKind::Rest,Activity::Rest).is_none());
        assert!(resolve(&packs,&reference,AnchorKind::Rest,Activity::MoveTo).unwrap().behavior.is_none());
    }
    #[test] fn conditions_read_confirmed_needs_without_changing_them() {
        use crate::life_domain::{ActorId,WorldId,Needs};
        let packs=packs();let object=packs[0].manifest.objects.iter().find(|object|object.kind==content_packs::ObjectKind::Rest).unwrap();
        let reference=ContentRef{package_id:"desk-garden".into(),package_version:1,template_id:object.id.clone()};
        let usage=resolve(&packs,&reference,AnchorKind::Rest,Activity::Rest).unwrap();
        let state=LifeState::new(ActorId::new("nova").unwrap(),WorldId::new("world_nova").unwrap(),Needs::new(9000,9000,9000,9000).unwrap());
        assert!(!condition(&usage,&state,&Default::default()));assert!(state.current_action().is_none());
    }
}
