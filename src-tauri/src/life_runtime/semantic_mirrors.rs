//! Read-only semantic mirrors. These Serialize-only rows identify a physical
//! owner; no row is an OutputProof, a NativeActionPermit or an execution ACK.
use super::{core::Core, LifeRuntime};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const ERROR: &str = "Die gemeinsame Spiegelansicht ist momentan nicht verfügbar.";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SemanticMirrorIdentity {
    pub render_session_id: String,
    pub binding_sequence: u64,
    pub surface_id: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SemanticMirrorOwner {
    pub actor_id: String,
    pub controlled: bool,
    pub owner: Option<SemanticMirrorIdentity>,
    pub world_instance_id: Option<String>,
    pub source_world_id: Option<String>,
    pub stage_id: Option<String>,
    pub config_revision: Option<u64>,
    pub visible: bool,
}
impl SemanticMirrorOwner {
    /// A retained constructor/loading/cleanup row suppresses a Studio owner.
    /// It cannot select a World epoch, render a pose or imply native Ready.
    fn suppressed(actor_id: String) -> Self {
        Self { actor_id, controlled: true, owner: None, world_instance_id: None,
            source_world_id: None, stage_id: None, config_revision: None, visible: false }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SemanticMirrorPacket {
    pub version: u16,
    pub epoch: String,
    pub sequence: u64,
    pub owners: Vec<SemanticMirrorOwner>,
    pub object_owner_actor_id: Option<String>,
}
impl SemanticMirrorPacket {
    /// OBS gets its existing bot scope only. A non-elected feed needs no
    /// identity for another actor to know it must omit the common object layer.
    pub(crate) fn for_actor(&self, actor: &str) -> Self {
        let mut packet = self.clone();
        packet.owners.retain(|owner| owner.actor_id == actor);
        packet.object_owner_actor_id = packet.object_owner_actor_id.filter(|owner| owner == actor);
        packet
    }
}

impl Core {
    fn semantic_mirror_packet(&mut self, controlled: BTreeSet<String>, now: u64) -> Result<SemanticMirrorPacket, String> {
        // The sequence advances for empty projections and availability changes,
        // independently of World position/snapshot sequence. Never wrap or reuse.
        let sequence = self.semantic_mirror_sequence.checked_add(1)
            .filter(|sequence| *sequence <= crate::life_domain::MAX_COUNTER)
            .ok_or(ERROR)?;
        let mut owners = self.world_outputs.semantic_mirrors(self, now).into_iter()
            .map(|owner| (owner.actor_id.clone(), owner)).collect::<BTreeMap<_, _>>();
        for actor in controlled {
            owners.entry(actor.clone()).or_insert_with(|| SemanticMirrorOwner::suppressed(actor));
        }
        let owners = owners.into_values().collect::<Vec<_>>();
        let object_owner_actor_id = owners.iter().find(|owner| owner.visible).map(|owner| owner.actor_id.clone());
        self.semantic_mirror_sequence = sequence;
        Ok(SemanticMirrorPacket { version: 1, epoch: self.semantic_mirror_epoch.clone(),
            sequence, owners, object_owner_actor_id })
    }
}

impl LifeRuntime {
    pub(crate) fn semantic_mirrors(&self) -> Result<SemanticMirrorPacket, String> {
        // Scene and SessionOutputs registries are observed and unlocked BEFORE
        // Core. Scene::begin can retain Scene -> Owned, while Owned -> Core is a
        // separate existing path. Never add Core -> Scene to that order.
        let mut controlled = self.session_outputs.retained_constructors()?.into_iter()
            .map(|receipt| receipt.bot().to_owned()).collect::<BTreeSet<_>>();
        for member in self.native_scenes.members(None) {
            if !member.cleanup_released() { controlled.insert(member.constructor.bot().to_owned()); }
        }
        let mut core = self.core.lock().map_err(|_| ERROR)?;
        core.semantic_mirror_packet(controlled, self.now())
    }
}

#[tauri::command(async)]
pub(crate) fn get_world_semantic_mirrors(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, crate::AppState>,
) -> Result<SemanticMirrorPacket, String> {
    if window.label() != "main" { return Err(ERROR.into()); }
    state.life_runtime.semantic_mirrors()
}

#[cfg(test)]
#[path = "semantic_mirrors_tests.rs"]
mod tests;
