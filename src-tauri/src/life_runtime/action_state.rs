//! Core-owned ephemeral action state. No providers, OS queries or persistence.
use crate::{
    action_executor::{Executor, Failure, JobKey, NativeAuthority},
    life_domain::{ActorId, CancelReason, WorldId, MAX_COUNTER},
    world_state::{coordinates::Point, SessionStamp, StageId},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UiProof {
    pub connection_id: String,
    pub sequence: u64,
}
#[derive(Clone)]
pub(crate) struct RestIntent {
    pub native: Option<super::native_scene_actions::NativeIntent>,
    pub move_target: Option<crate::world_state::coordinates::Point>,
    pub serial: u64,
    pub bot: String,
    pub session: String,
    pub revision: u64,
    pub profile: Value,
    pub connection: String,
    pub report_sequence: u64,
    pub reported_at: u64,
    pub issued_at: u64,
    pub own_ui_generation: u64,
    pub output_generation: u64,
}
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct LocalFence {
    pub session: String,
    pub revision: u64,
    pub system_generation: u64,
    pub ui_generation: u64,
}
/// Checked native World observation, never a renderer's pixel-arrival claim.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct WorldActionFence {
    pub session: SessionStamp,
    pub source_world: WorldId,
    pub stage: StageId,
    pub config_revision: u64,
    pub topology_revision: u64,
    pub position: Point,
    pub instance_id: String,
    pub record: Value,
}
#[derive(Clone)]
pub(crate) struct NativeJob {
    pub visual: Option<crate::world_output::VisualExecution>,
    pub native_permit: Option<super::native_scene::NativeActionPermit>,
    pub native_rest_output: Option<crate::world_output::OutputProof>,
    pub slot: Option<crate::world_hub::SlotReservation>,
    pub authority: NativeAuthority,
    pub key: JobKey,
    pub fence: LocalFence,
    pub world: WorldActionFence,
    pub profile: Value,
    pub persisted_profile: crate::storage::action_store::ProfileToken,
    pub cancel: Arc<AtomicBool>,
}
pub(crate) struct NativeActions {
    pub executor: Executor,
    pub jobs: BTreeMap<String, NativeJob>,
    pub instance_id: String,
    pub sequence: u64,
    pub retired: std::collections::VecDeque<crate::action_executor::JobSnapshot>,
}
impl NativeActions {
    pub fn new() -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        Self {
            executor: Executor::new(id.clone()).expect("native UUID"),
            jobs: BTreeMap::new(),
            instance_id: id,
            sequence: 1,
            retired: std::collections::VecDeque::new(),
        }
    }
    pub fn changed(&mut self) {
        self.sequence = self.sequence.saturating_add(1).min(MAX_COUNTER);
    }
    pub fn busy(&self) -> bool {
        self.jobs.values().any(|job| {
            self.executor.status(&job.key).is_some_and(|status| {
                !matches!(
                    status,
                    crate::action_executor::Status::Completed
                        | crate::action_executor::Status::Cancelled
                        | crate::action_executor::Status::Failed
                )
            })
        })
    }
    pub fn interrupt(
        &mut self,
        bot: Option<&str>,
        failure: Option<Failure>,
        reason: CancelReason,
        now: u64,
    ) {
        for (id, job) in &self.jobs {
            if bot.is_some_and(|bot| bot != id) {
                continue;
            }
            job.cancel.store(true, Ordering::SeqCst);
            let _ = if let Some(failure) = failure {
                self.executor.fail(&job.authority, &job.key, failure, now)
            } else {
                self.executor.cancel(&job.authority, &job.key, reason, now)
            };
        }
        self.changed();
    }
    pub fn actor_busy(&self, id: &str) -> bool {
        self.jobs.get(id).is_some_and(|job| {
            self.executor.status(&job.key).is_some_and(|status| {
                !matches!(
                    status,
                    crate::action_executor::Status::Completed
                        | crate::action_executor::Status::Cancelled
                        | crate::action_executor::Status::Failed
                )
            })
        })
    }
    pub fn forget(&mut self, id: &str) {
        if let Ok(actor) = ActorId::new(id) {
            if self.executor.forget_terminal(&actor) {
                self.jobs.remove(id);
                self.changed();
            }
        }
    }
}
