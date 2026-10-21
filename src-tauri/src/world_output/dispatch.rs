//! Routing descriptors retain the exact host-issued transport. This module
//! never creates NativeRouteProofs, action permits or renderer receipts.
use super::{Registry, SourceRoute};
use crate::{life_runtime::native_scene::NativeRouteProof, world_output::{BindingStamp, MotionCommand, MotionFence}};

pub(crate) enum RoutedWorldCommand {
    Studio(MotionCommand),
    Native { command: MotionCommand, route: NativeRouteProof, issued_at: u64 },
    Unavailable(MotionCommand),
}

fn stamp_matches(fence: &MotionFence, stamp: &BindingStamp) -> bool {
    fence.actor_id == stamp.actor_id && fence.action_id.actor_id().as_str() == stamp.actor_id
        && fence.action_id.world_id().as_str() == stamp.source_world_id
        && fence.render_session_id == stamp.render_session_id
        && fence.binding_sequence == stamp.binding_sequence
        && fence.world_instance_id == stamp.world_instance_id && fence.stage_id == stamp.stage_id
        && fence.config_revision == stamp.config_revision && fence.topology_revision == stamp.topology_revision
        && fence.layout_revision == stamp.layout_revision && fence.asset_revision == stamp.asset_revision
        && fence.measurement_revision == stamp.measurement_revision && fence.reference_revision == stamp.reference_revision
}

impl Registry {
    /// Drain while Core is held. A later replacement can only reject this
    /// frozen route; it cannot relabel old work as a successor's transport.
    pub(crate) fn take_routed_command(&mut self, now: u64) -> Option<RoutedWorldCommand> {
        let command = self.outbox.pop_front()?;
        let Some(entry) = self.entries.get(&command.fence().actor_id)
            .filter(|entry| stamp_matches(command.fence(), &entry.stamp)) else {
            return Some(RoutedWorldCommand::Unavailable(command));
        };
        Some(match &entry.source.route {
            SourceRoute::Studio => RoutedWorldCommand::Studio(command),
            SourceRoute::Native(route) => RoutedWorldCommand::Native {
                command, route: route.clone(), issued_at: now,
            },
        })
    }
    /// Pure exact-attachment check for the later Owned -> Core validation.
    /// This conveys no action admission and performs no GUI operation.
    pub(crate) fn native_command_current(&self, command: &MotionCommand, route: &NativeRouteProof) -> bool {
        self.entries.get(&command.fence().actor_id).is_some_and(|entry|
            stamp_matches(command.fence(), &entry.stamp)
                && matches!(&entry.source.route, SourceRoute::Native(current) if current.same_route(route)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life_domain::{ActionId, ActorId, WorldId};
    fn stamp() -> BindingStamp {
        BindingStamp { actor_id:"nova".into(), source_world_id:"life-world".into(), render_session_id:"render".into(),
            binding_sequence:1, world_instance_id:"world-instance".into(), stage_id:"stage".into(), config_revision:2,
            topology_revision:3, layout_revision:4, asset_revision:5, measurement_revision:6, reference_revision:7 }
    }
    fn fence() -> MotionFence {
        let s=stamp();
        MotionFence { actor_id:s.actor_id, render_session_id:s.render_session_id, binding_sequence:s.binding_sequence,
            world_instance_id:s.world_instance_id, stage_id:s.stage_id, config_revision:s.config_revision,
            topology_revision:s.topology_revision, layout_revision:s.layout_revision, asset_revision:s.asset_revision,
            measurement_revision:s.measurement_revision, reference_revision:s.reference_revision,
            action_id:ActionId::new(ActorId::new("nova").unwrap(), WorldId::new("life-world").unwrap(), 1).unwrap(),
            delivery_id:"delivery".into() }
    }
    #[test]
    fn route_identity_requires_source_lineage_and_all_binding_revisions() {
        let s=stamp(); let f=fence(); assert!(stamp_matches(&f,&s));
        let mut changed=f.clone(); changed.reference_revision+=1; assert!(!stamp_matches(&changed,&s));
        let mut changed=f.clone(); changed.measurement_revision+=1; assert!(!stamp_matches(&changed,&s));
        let mut changed=f.clone(); changed.binding_sequence+=1; assert!(!stamp_matches(&changed,&s));
        let mut changed=f.clone(); changed.world_instance_id="other".into(); assert!(!stamp_matches(&changed,&s));
        let mut changed=s.clone(); changed.source_world_id="other-life".into(); assert!(!stamp_matches(&f,&changed));
        let mut changed=s; changed.actor_id="other".into(); assert!(!stamp_matches(&f,&changed));
    }
    #[test]
    fn absent_registry_route_is_unavailable_instead_of_falling_back_to_studio() {
        let mut registry=Registry::default();
        registry.enqueue(MotionCommand::Pause { version:1, sequence:1, fence:fence() }).unwrap();
        assert!(matches!(registry.take_routed_command(10),Some(RoutedWorldCommand::Unavailable(_))));
        assert!(registry.take_routed_command(11).is_none());
    }
}
