//! A Serialize-only observation of the retained native Registry attachment.
//! No constructor, document capability or Native proof is minted here.
use super::{Registry, SourceRoute, TTL_MS};
use crate::life_runtime::{core::Core, semantic_mirrors::{SemanticMirrorIdentity, SemanticMirrorOwner}};
use std::sync::atomic::Ordering;

fn visible(cancelled: bool, owner: bool, bound: bool, current: bool, live: bool,
    frame: u64, observed: Option<u64>, now: u64) -> bool {
    // A read-only projection follows the actual Live owner and its accepted
    // binding lifetime. FRAME_MS bounds newly received evidence; it is not an
    // idle retention lease and must not require continuous calibration IPC.
    !cancelled && owner && bound && current && live && frame > 0
        && observed.is_some_and(|received| now >= received && now - received <= TTL_MS)
}

impl Registry {
    pub(crate) fn semantic_mirrors(&self, core: &Core, now: u64) -> Vec<SemanticMirrorOwner> {
        self.entries.iter().filter_map(|(actor, entry)| {
            let SourceRoute::Native(route) = &entry.source.route else { return None; };
            // Revocation/expiration does not remove the physical cleanup barrier.
            // A released route is no longer a controlled native surface.
            if !route.retained() { return None; }
            Some(SemanticMirrorOwner {
                actor_id: actor.clone(), controlled: true,
                owner: Some(SemanticMirrorIdentity {
                    render_session_id: entry.stamp.render_session_id.clone(),
                    binding_sequence: entry.stamp.binding_sequence,
                    surface_id: entry.surface.clone(),
                }),
                world_instance_id: Some(entry.stamp.world_instance_id.clone()),
                source_world_id: Some(entry.stamp.source_world_id.clone()),
                stage_id: Some(entry.stamp.stage_id.clone()),
                config_revision: Some(entry.stamp.config_revision),
                visible: visible(entry.cancel.load(Ordering::SeqCst), entry.source.owner,
                    entry.binding.is_some(), route.context_current(core, now), route.live_current(now),
                    entry.frame, entry.observed, now),
            })
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mirror_retains_a_valid_idle_owner_past_receipt_freshness_until_binding_expiry() {
        assert_eq!(TTL_MS, 5_000);
        // No new frame is fabricated: the same accepted observation remains
        // suitable for display across the existing three-second idle cadence.
        for now in [350, 351, 3_099, 3_100, 5_099, 5_100] {
            assert!(visible(false, true, true, true, true, 3, Some(100), now));
        }
        assert!(!visible(false, true, true, true, true, 3, Some(100), 5_101));
    }
    #[test]
    fn metadata_does_not_retime_an_observation_and_future_or_missing_frames_stay_hidden() {
        let observed = Some(100);
        assert!(visible(false, true, true, true, true, 3, observed, 3_100));
        // Repeated projections reuse received=100; none extends its expiry.
        assert!(!visible(false, true, true, true, true, 3, observed, 5_101));
        assert!(!visible(false, true, true, true, true, 3, observed, 8_100));
        assert!(!visible(false, true, true, true, true, 3, Some(101), 100));
        assert!(!visible(false, true, true, true, true, 0, Some(100), 100));
        assert!(!visible(false, true, true, true, true, 3, None, 100));
    }
    #[test]
    fn cancellation_missing_binding_and_pending_or_stale_context_are_not_visible() {
        assert!(!visible(true, true, true, true, true, 3, Some(100), 100));
        assert!(!visible(false, false, true, true, true, 3, Some(100), 100));
        assert!(!visible(false, true, false, true, true, 3, Some(100), 100));
        assert!(!visible(false, true, true, false, true, 3, Some(100), 100));
        assert!(!visible(false, true, true, true, false, 3, Some(100), 100));
    }
    #[test]
    fn retained_age_never_overrides_current_owner_or_core_fences() {
        // A previously valid observation remains inside TTL, but any exact
        // cancellation, missing binding, pending/restoring route or changed
        // Core context hides it immediately on the next host projection.
        let now = 3_100;
        assert!(!visible(true, true, true, true, true, 3, Some(100), now));
        assert!(!visible(false, true, false, true, true, 3, Some(100), now));
        assert!(!visible(false, true, true, false, true, 3, Some(100), now));
        assert!(!visible(false, true, true, true, false, 3, Some(100), now));
    }
}
