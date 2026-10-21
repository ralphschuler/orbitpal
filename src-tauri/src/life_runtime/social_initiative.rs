//! Advisory scene proposals over the host's actual current actors/free slots.
//! Every chosen proposal must still pass request(), coordinator and native Plan.
use super::{choose_slots, Actor};
use crate::{character_development::HobbyKind, life_domain::MAX_COUNTER, social::SceneKind};
use chrono::Timelike;
use std::collections::BTreeMap;

pub(super) const MAX_ATTEMPTS: usize = 8;
const INTERVAL_MS: u64 = 60_000;
const MAX_HISTORY: usize = 64;
type Key=(String,String,u8);
#[derive(Clone)]
pub(super) struct Proposal {
    pub bot: String,
    pub target: String,
    pub kind: SceneKind,
    key: Key,
    pressure: u32,
    preference: u32,
    cooldown: u64,
}
#[derive(Default)]
struct Attempt { sequence:u64, retry_after:u64 }
#[derive(Default)]
struct Served { sequence:u64, next:u64 }
#[derive(Default)]
pub(super) struct Memory {
    sequence:u64,
    attempts:BTreeMap<Key,Attempt>,
    served:BTreeMap<String,Served>,
}
pub(super) fn local_minute(epoch:u64)->Option<u16> {
    let instant=chrono::DateTime::<chrono::Utc>::from_timestamp_millis(i64::try_from(epoch).ok()?)?;
    let local=instant.with_timezone(&chrono::Local);
    Some((local.hour()*60+local.minute()) as u16)
}
fn relevance(actor:&Actor,kind:SceneKind,minute:u16)->(u32,u32) {
    let character=&actor.participant.character;
    let needs=actor.participant.needs;
    let (level,threshold,preference):(u16,u16,u32)=match kind {
        SceneKind::Greeting=>(needs.social().units(),6000,
            (u32::from(character.traits.sociability)+u32::from(character.preference(HobbyKind::Company,minute)))/2),
        SceneKind::Play=>(needs.engagement().units(),6500,u32::from(character.preference(HobbyKind::Play,minute))),
        SceneKind::Rest=>(needs.energy().units(),6500,u32::from(character.traits.rest_preference)),
    };
    (u32::from(threshold.saturating_sub(level)),preference)
}
impl Memory {
    fn allowed(&self, proposal:&Proposal,now:u64)->bool {
        self.attempts.get(&proposal.key).is_none_or(|attempt|now>=attempt.retry_after)
            && [&proposal.bot,&proposal.target].iter().all(|id|self.served.get(*id).is_none_or(|served|now>=served.next))
            && (self.attempts.contains_key(&proposal.key)||self.attempts.len()<MAX_HISTORY)
            && self.served.len()+[&proposal.bot,&proposal.target].iter().filter(|id|!self.served.contains_key(id.as_str())).count()<=MAX_HISTORY
    }
    pub(super) fn current(&self,proposal:&Proposal,now:u64)->bool {now<=MAX_COUNTER&&self.allowed(proposal,now)}
    pub(super) fn proposals(&mut self,actors:&[Actor],minute:u16,now:u64)->Vec<Proposal> {
        if actors.len()>crate::world_output::MAX_LAYERS || minute>=1440 || now>MAX_COUNTER {return Vec::new();}
        let present=|id:&str|actors.iter().any(|actor|actor.participant.actor.as_str()==id);
        // A transient UI/system/output observation gap is not a session end.
        // Retain every unexpired operator limit even while both actors vanish.
        self.served.retain(|id,served|present(id)||now<served.next);
        self.attempts.retain(|(a,b,_),attempt|(present(a)&&present(b))||now<attempt.retry_after);
        if self.attempts.len()>=MAX_HISTORY {
            if let Some(key)=self.attempts.iter().filter(|(_,attempt)|now>=attempt.retry_after).min_by_key(|(_,attempt)|attempt.sequence).map(|(key,_)|key.clone()) {
                self.attempts.remove(&key);
            }
        }
        let mut proposals=Vec::new();
        for (index,a) in actors.iter().enumerate() {
            for b in actors.iter().skip(index+1) {
                if [a,b].iter().any(|actor|!actor.participant.available||!actor.station_supported
                    ||actor.participant.needs.energy().units()<2500||actor.participant.needs.nourishment().units()<2000) {continue;}
                let initiative=|actor:&Actor|actor.participant.character.traits.initiative;
                let (inviter,guest)=if initiative(a)>=initiative(b) {(a,b)} else {(b,a)};
                if initiative(inviter)<500 {continue;}
                for (rank,kind) in [SceneKind::Greeting,SceneKind::Play,SceneKind::Rest].into_iter().enumerate() {
                    if choose_slots(a,b,kind).is_none() {continue;}
                    let (pa,wa)=relevance(a,kind,minute);let (pb,wb)=relevance(b,kind,minute);
                    let preference=(wa+wb)/2;
                    // Genuine need wins. Strong explicit interests/routines can
                    // also propose voluntary play without manufacturing need.
                    if pa+pb==0 && preference<750 {continue;}
                    let mut pair=[a.participant.actor.as_str().to_owned(),b.participant.actor.as_str().to_owned()];pair.sort();
                    let proposal=Proposal{bot:inviter.participant.actor.as_str().into(),target:guest.participant.actor.as_str().into(),kind,
                        key:(pair[0].clone(),pair[1].clone(),rank as u8),pressure:pa+pb,preference,
                        cooldown:inviter.initiative_cooldown_ms.max(guest.initiative_cooldown_ms).max(INTERVAL_MS)};
                    if self.allowed(&proposal,now) {proposals.push(proposal);}
                }
            }
        }
        let served=|proposal:&Proposal|[&proposal.bot,&proposal.target].iter().filter_map(|id|self.served.get(*id)).map(|served|served.sequence).max().unwrap_or(0);
        let attempted=|proposal:&Proposal|self.attempts.get(&proposal.key).map_or(0,|attempt|attempt.sequence);
        proposals.sort_by(|a,b|b.pressure.cmp(&a.pressure).then(served(a).cmp(&served(b)))
            .then(b.preference.cmp(&a.preference)).then(attempted(a).cmp(&attempted(b))).then(a.key.cmp(&b.key)));
        proposals.truncate(MAX_ATTEMPTS);
        proposals
    }
    pub(super) fn record(&mut self,proposal:&Proposal,now:u64,accepted:bool) {
        if !self.current(proposal,now) {return;}
        self.sequence=self.sequence.saturating_add(1).min(MAX_COUNTER);
        self.attempts.insert(proposal.key.clone(),Attempt{sequence:self.sequence,retry_after:now.saturating_add(proposal.cooldown).min(MAX_COUNTER)});
        for id in [&proposal.bot,&proposal.target] {
            let served=self.served.entry(id.clone()).or_default();served.sequence=self.sequence;
            if accepted {served.next=now.saturating_add(proposal.cooldown).min(MAX_COUNTER);}
        }
        // Capacity refuses new proposals instead of evicting a live cooldown.
        debug_assert!(self.attempts.len()<=MAX_HISTORY&&self.served.len()<=MAX_HISTORY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{character_development::{Character,Routine},life_domain::{ActorId,LifeState,Needs,WorldId},
        social::Participant,world_hub::ActionSlotTarget,world_state::{AnchorKind,SlotId,coordinates::Point}};
    fn actor(id:&str,energy:u16,engagement:u16,social:u16)->Actor {
        let life=LifeState::new(ActorId::new(id).unwrap(),WorldId::new(format!("world_{id}")).unwrap(),Needs::new(energy,9000,engagement,social).unwrap());
        let slots=[AnchorKind::Meeting,AnchorKind::Play,AnchorKind::Rest].into_iter().enumerate().flat_map(|(kind_index,kind)|
            (0..2).map(move|index|ActionSlotTarget{id:SlotId::new(format!("slot_{kind_index}_{index}")).unwrap(),kind,position:Point::new(400_000+index*100_000,500_000).unwrap(),content_ref:None})).collect::<Vec<_>>();
        let groups=slots.chunks(2).map(|slots|slots.iter().map(|slot|slot.id.clone()).collect()).collect();
        Actor{participant:Participant{actor:life.actor_id().clone(),world:life.world_id().clone(),session:format!("session_{id}"),generation:1,available:true,needs:life.needs(),character:Character::default(),familiarity:0,affection:0},
            scope:super::super::Scope{instance:"actual_world".into(),revision:1,topology:1},slots,groups,life,station_supported:true,initiative_cooldown_ms:60_000}
    }
    #[test]
    fn actual_needs_choose_play_rest_and_greeting_instead_of_a_fixed_scene() {
        for (energy,engagement,social,wanted) in [(9000,4000,9000,SceneKind::Play),(3000,9000,9000,SceneKind::Rest),(9000,9000,4000,SceneKind::Greeting)] {
            let actors=vec![actor("a",energy,engagement,social),actor("b",energy,engagement,social)];
            assert_eq!(Memory::default().proposals(&actors,540,0)[0].kind,wanted);
        }
    }
    #[test]
    fn a_higher_sorted_eligible_initiator_is_not_lost_and_critical_care_still_wins() {
        let mut actors=vec![actor("a",9000,4000,9000),actor("z",9000,4000,9000)];
        actors[0].participant.character.traits.initiative=100;actors[1].participant.character.traits.initiative=900;
        assert_eq!(Memory::default().proposals(&actors,0,0)[0].bot,"z");
        actors[1].participant.needs=Needs::new(2000,9000,4000,9000).unwrap();
        assert!(Memory::default().proposals(&actors,0,0).is_empty());
    }
    #[test]
    fn source_scope_capacity_and_real_station_capability_cannot_be_replaced_by_preference() {
        let a=actor("a",9000,4000,9000);let mut b=actor("b",9000,4000,9000);
        b.scope.revision=2;assert!(Memory::default().proposals(&[a.clone(),b.clone()],0,0).is_empty());
        b.scope=a.scope.clone();b.station_supported=false;assert!(Memory::default().proposals(&[a.clone(),b.clone()],0,0).is_empty());
        b.station_supported=true;b.slots.retain(|slot|slot.kind!=AnchorKind::Play||slot.id.as_str().ends_with("_0"));
        let mut a=a;a.slots.retain(|slot|slot.kind!=AnchorKind::Play||slot.id.as_str().ends_with("_0"));
        assert!(Memory::default().proposals(&[a,b],0,0).is_empty(),"a one-capacity toy cannot serve two scene roles");
    }
    #[test]
    fn local_routines_and_interests_can_propose_voluntary_play_without_changing_needs() {
        let mut actors=vec![actor("a",9000,9000,9000),actor("b",9000,9000,9000)];
        for actor in &mut actors {actor.participant.character.routines.push(Routine{start_minute:540,end_minute:600,kind:HobbyKind::Play,weight:500});}
        let before=actors[0].life.clone();
        assert!(Memory::default().proposals(&actors,480,0).is_empty());
        assert_eq!(Memory::default().proposals(&actors,570,0)[0].kind,SceneKind::Play);
        assert_eq!(actors[0].life,before);
    }
    #[test]
    fn equal_need_pairs_rotate_participation_and_honor_success_cooldown_and_failure_backoff() {
        let actors=vec![actor("a",9000,4000,9000),actor("b",9000,4000,9000),actor("c",9000,4000,9000)];
        let mut memory=Memory::default();let first=memory.proposals(&actors,0,0)[0].clone();
        memory.record(&first,0,true);assert!(memory.proposals(&actors,0,59_999).is_empty());
        let next=memory.proposals(&actors,0,60_000)[0].clone();assert!(next.bot=="c"||next.target=="c");
        memory.record(&next,60_000,false);
        assert!(!memory.current(&next,119_999));assert!(memory.current(&next,120_000));
        assert!(memory.proposals(&actors,0,60_000).len()<=MAX_ATTEMPTS);
    }
    #[test]
    fn operator_cooldown_and_missing_partners_keep_the_advisory_memory_bounded() {
        let mut actors=vec![actor("a",9000,4000,9000),actor("b",9000,4000,9000)];actors[1].initiative_cooldown_ms=300_000;
        let mut memory=Memory::default();let proposal=memory.proposals(&actors,0,0)[0].clone();memory.record(&proposal,0,true);
        assert!(memory.proposals(&actors,0,299_999).is_empty());assert!(!memory.proposals(&actors,0,300_000).is_empty());
        assert!(memory.proposals(&actors[..1],0,300_000).is_empty());assert!(memory.attempts.is_empty());
    }
    #[test]
    fn temporary_absence_cannot_reset_an_operator_cooldown_or_failed_pair_backoff() {
        let mut actors=vec![actor("a",9000,4000,9000),actor("b",9000,4000,9000)];
        actors[0].initiative_cooldown_ms=300_000;
        let mut memory=Memory::default();let proposal=memory.proposals(&actors,0,0)[0].clone();
        memory.record(&proposal,0,true);
        assert!(memory.proposals(&[],0,60_000).is_empty());
        assert_eq!(memory.served.len(),2);assert_eq!(memory.attempts.len(),1);
        assert!(memory.proposals(&actors,0,120_000).is_empty());
        assert!(!memory.proposals(&actors,0,300_000).is_empty());
        let mut failed=Memory::default();let proposal=failed.proposals(&actors,0,0)[0].clone();
        failed.record(&proposal,0,false);
        failed.proposals(&[],0,60_000);
        assert!(!failed.current(&proposal,299_999));
        assert!(failed.current(&proposal,300_000));
    }
    #[test]
    fn live_limits_are_never_evicted_to_admit_replacement_actors() {
        let mut memory=Memory::default();
        for index in 0..MAX_HISTORY/2 {
            let actors=vec![actor(&format!("a{index}"),9000,4000,9000),actor(&format!("b{index}"),9000,4000,9000)];
            let proposal=memory.proposals(&actors,0,0)[0].clone();memory.record(&proposal,0,true);
        }
        let actors=vec![actor("replacement_a",9000,4000,9000),actor("replacement_b",9000,4000,9000)];
        assert_eq!(memory.served.len(),MAX_HISTORY);
        assert!(memory.proposals(&actors,0,59_999).is_empty());
        assert!(!memory.proposals(&actors,0,60_000).is_empty());
        assert!(memory.attempts.len()<=MAX_HISTORY&&memory.served.len()<=MAX_HISTORY);
    }
}
