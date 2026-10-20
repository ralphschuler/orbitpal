//! Closed, non-authorizing Studio World render protocol. No private Life session,
//! native window handle, credential, capture grant or Store permit is serialized.
use crate::life_domain::{ActionId, ActionPhase, MAX_COUNTER};
use serde::{Deserialize, Serialize};
pub(crate) const TTL_MS: u64 = 5_000;
pub(crate) const FRAME_MS: u64 = 250;
pub(crate) const MAX_LAYERS: usize = 8;
pub(crate) const MAX_OUTBOX: usize = 256;
pub(crate) const MAX_DIM: f64 = 1_000_000.0;
pub(crate) fn counter(n: u64) -> bool {
    n > 0 && n <= MAX_COUNTER
}
pub(crate) fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}
pub(crate) fn finite(n: f64) -> bool {
    n.is_finite() && n.abs() <= MAX_DIM
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CssPoint {
    pub x: f64,
    pub y: f64,
}
impl CssPoint {
    pub fn valid(self) -> bool {
        finite(self.x) && finite(self.y)
    }
    pub fn distance(self, p: Self) -> f64 {
        (self.x - p.x).hypot(self.y - p.y)
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CssSize {
    pub width: f64,
    pub height: f64,
}
impl CssSize {
    pub fn valid(self) -> bool {
        finite(self.width) && finite(self.height) && self.width > 0.0 && self.height > 0.0
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CssRect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}
impl CssRect {
    pub fn valid(self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .into_iter()
            .all(finite)
            && self.left < self.right
            && self.top < self.bottom
    }
    pub fn contains(self, p: CssPoint) -> bool {
        p.x >= self.left && p.x <= self.right && p.y >= self.top && p.y <= self.bottom
    }
    pub fn contains_rect(self, b: Self) -> bool {
        b.left >= self.left && b.top >= self.top && b.right <= self.right && b.bottom <= self.bottom
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EnvelopeExtents {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}
impl EnvelopeExtents {
    pub fn valid(self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .into_iter()
            .all(|n| finite(n) && n >= 0.0)
            && self.left + self.right > 0.0
            && self.top + self.bottom > 0.0
    }
    pub fn bounds(self, a: CssPoint) -> CssRect {
        CssRect {
            left: a.x - self.left,
            top: a.y - self.top,
            right: a.x + self.right,
            bottom: a.y + self.bottom,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LocalReference {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl LocalReference {
    pub fn valid(self) -> bool {
        [self.x, self.y, self.z].into_iter().all(finite)
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GroundVector {
    pub x: f64,
    pub z: f64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GroundBasis {
    pub right: GroundVector,
    pub down: GroundVector,
}
impl GroundBasis {
    pub fn valid(self) -> bool {
        [self.right.x, self.right.z, self.down.x, self.down.z]
            .into_iter()
            .all(|n| n.is_finite() && n.abs() <= 10_000.0)
            && (self.right.x * self.down.z - self.right.z * self.down.x).abs() > 1e-12
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RenderReason {
    Unregistered,
    ScopeMismatch,
    StaleBinding,
    UnsupportedOutput,
    UnsupportedCapability,
    LayoutUnconfirmed,
    MeasurementUnavailable,
    TargetDoesNotFit,
    Expired,
    Cancelled,
    StorageUnavailable,
    Capacity,
    ProtocolError,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BindingStamp {
    pub actor_id: String,
    pub source_world_id: String,
    pub render_session_id: String,
    pub binding_sequence: u64,
    pub world_instance_id: String,
    pub stage_id: String,
    pub config_revision: u64,
    pub topology_revision: u64,
    pub layout_revision: u64,
    pub asset_revision: u64,
    pub measurement_revision: u64,
    pub reference_revision: u64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CameraPolicy {
    WorldIsoV1,
}
/// Encloses current CPU-submitted source geometry under the current Three.js
/// Float64 graph. This is not tight extrema, shader/GPU or visible-pixel proof.
/// No fallback/default: older or unknown measurement policies must be rejected.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) enum EnclosureKind {
    #[serde(rename = "conservative_submitted_geometry_v1")]
    ConservativeSubmittedGeometryV1,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MeasurementChallenge {
    pub id: String,
    pub expires_in_ms: u16,
    pub expected: BindingStamp,
    pub camera_policy: CameraPolicy,
    pub enclosure_kind: EnclosureKind,
    pub profile_scale: f64,
    pub effective_scale: f64,
    pub expected_reference_local: Option<LocalReference>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MotionBinding {
    pub actor_id: String,
    pub source_world_id: String,
    pub render_session_id: String,
    pub binding_sequence: u64,
    pub world_instance_id: String,
    pub stage_id: String,
    pub config_revision: u64,
    pub topology_revision: u64,
    pub layout_revision: u64,
    pub asset_revision: u64,
    pub measurement_revision: u64,
    pub reference_revision: u64,
    pub enclosure_kind: EnclosureKind,
    pub viewport: CssSize,
    pub safe_centers: CssRect,
    pub maximum_footprint: CssSize,
    pub envelope_extents: EnvelopeExtents,
    pub reference_local: LocalReference,
    pub position: CssPoint,
    pub ground_basis: GroundBasis,
    pub model_forward_yaw: f64,
    pub profile_scale: f64,
    pub effective_scale: f64,
    pub scale: f64,
    pub reduced_motion: bool,
    pub walk_supported: bool,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RenderRole {
    OwnerCandidate,
    Mirror,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Readiness {
    AwaitingLayout,
    AwaitingMeasurement { challenge: MeasurementChallenge },
    Ready { binding: MotionBinding },
    Unavailable { reason: RenderReason },
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RegisteredWorldActor {
    pub version: u16,
    pub render_session_id: String,
    pub binding_sequence: u64,
    pub surface_id: String,
    pub role: RenderRole,
    pub readiness: Readiness,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RegisterWorldActor {
    pub version: u16,
    pub mount_id: String,
    pub actor_id: String,
    pub world_instance_id: String,
    pub stage_id: String,
    pub config_revision: u64,
    pub studio_proof: Option<crate::life_runtime::action_state::UiProof>,
}
impl RegisterWorldActor {
    pub fn valid(&self) -> bool {
        self.version == 1
            && [
                &self.mount_id,
                &self.actor_id,
                &self.world_instance_id,
                &self.stage_id,
            ]
            .into_iter()
            .all(|v| id(v))
            && counter(self.config_revision)
            && self
                .studio_proof
                .as_ref()
                .is_some_and(|p| id(&p.connection_id) && counter(p.sequence))
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RenderMeasurement {
    pub version: u16,
    pub challenge_id: String,
    pub enclosure_kind: EnclosureKind,
    pub frame_sequence: u64,
    pub viewport: CssSize,
    pub clip: CssRect,
    pub anchor: CssPoint,
    /// Enclosure at the submitted pose; not necessarily tight or raster-visible.
    pub posed_bounds: CssRect,
    pub envelope_extents: EnvelopeExtents,
    pub reference_local: LocalReference,
    pub ground_basis: GroundBasis,
    pub model_forward_yaw: f64,
    pub applied_model_scale: f64,
    pub reduced_motion: bool,
    pub walk_supported: bool,
}
impl RenderMeasurement {
    pub fn valid(&self) -> bool {
        self.version == 1
            && id(&self.challenge_id)
            && counter(self.frame_sequence)
            && self.viewport.valid()
            && self.clip.valid()
            && self.anchor.valid()
            && self.posed_bounds.valid()
            && self.envelope_extents.valid()
            && self.reference_local.valid()
            && self.ground_basis.valid()
            && self.model_forward_yaw.is_finite()
            && self.model_forward_yaw.abs() <= std::f64::consts::PI
            && self.applied_model_scale.is_finite()
            && (0.01..=100.0).contains(&self.applied_model_scale)
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MeasurementAck {
    pub version: u16,
    pub frame_sequence: u64,
    pub output: RegisteredWorldActor,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MotionFence {
    pub actor_id: String,
    pub render_session_id: String,
    pub binding_sequence: u64,
    pub world_instance_id: String,
    pub stage_id: String,
    pub config_revision: u64,
    pub topology_revision: u64,
    pub layout_revision: u64,
    pub asset_revision: u64,
    pub measurement_revision: u64,
    pub reference_revision: u64,
    pub action_id: ActionId,
    pub delivery_id: String,
}
impl MotionFence {
    pub fn valid(&self) -> bool {
        [
            &self.actor_id,
            &self.render_session_id,
            &self.world_instance_id,
            &self.stage_id,
            &self.delivery_id,
        ]
        .into_iter()
        .all(|v| id(v))
            && [
                self.binding_sequence,
                self.config_revision,
                self.topology_revision,
                self.layout_revision,
                self.asset_revision,
                self.measurement_revision,
                self.reference_revision,
            ]
            .into_iter()
            .all(counter)
            && self.action_id.actor_id().as_str() == self.actor_id
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FrameObservation {
    pub frame_sequence: u64,
    pub enclosure_kind: EnclosureKind,
    pub viewport: CssSize,
    pub clip: CssRect,
    pub anchor: CssPoint,
    /// Same frozen enclosure contract as the native-issued binding.
    pub posed_bounds: CssRect,
    pub walking: bool,
    pub target_idle_frames: u16,
}
impl FrameObservation {
    pub fn valid(&self) -> bool {
        counter(self.frame_sequence)
            && self.viewport.valid()
            && self.clip.valid()
            && self.anchor.valid()
            && self.posed_bounds.valid()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum ActionReport {
    Delivered {
        version: u16,
        fence: MotionFence,
        receipt_sequence: u64,
        frame: FrameObservation,
    },
    Started {
        version: u16,
        fence: MotionFence,
        receipt_sequence: u64,
        frame: FrameObservation,
    },
    Progress {
        version: u16,
        fence: MotionFence,
        receipt_sequence: u64,
        frame: FrameObservation,
    },
    Completed {
        version: u16,
        fence: MotionFence,
        receipt_sequence: u64,
        frame: FrameObservation,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReportKind {
    Delivered,
    Started,
    Progress,
    Completed,
}
impl ActionReport {
    pub fn parts(&self) -> (u16, &MotionFence, u64, &FrameObservation, ReportKind) {
        match self {
            Self::Delivered {
                version,
                fence,
                receipt_sequence,
                frame,
            } => (
                *version,
                fence,
                *receipt_sequence,
                frame,
                ReportKind::Delivered,
            ),
            Self::Started {
                version,
                fence,
                receipt_sequence,
                frame,
            } => (
                *version,
                fence,
                *receipt_sequence,
                frame,
                ReportKind::Started,
            ),
            Self::Progress {
                version,
                fence,
                receipt_sequence,
                frame,
            } => (
                *version,
                fence,
                *receipt_sequence,
                frame,
                ReportKind::Progress,
            ),
            Self::Completed {
                version,
                fence,
                receipt_sequence,
                frame,
            } => (
                *version,
                fence,
                *receipt_sequence,
                frame,
                ReportKind::Completed,
            ),
        }
    }
    pub fn valid(&self) -> bool {
        let (v, f, s, p, _) = self.parts();
        v == 1 && f.valid() && counter(s) && p.valid()
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReportOutcome {
    Accepted,
    Duplicate,
    Cancelled,
    Rejected,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ActionReportAck {
    pub version: u16,
    pub receipt_sequence: u64,
    pub outcome: ReportOutcome,
    pub runtime_status: Option<crate::action_executor::Status>,
    pub confirmed_phase: Option<ActionPhase>,
    pub reason: Option<RenderReason>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UnregisterWorldActor {
    pub version: u16,
    pub render_session_id: String,
    pub binding_sequence: u64,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StartLifeMove {
    pub bot_id: String,
    pub target: crate::world_state::coordinates::Point,
    pub ui: crate::life_runtime::action_state::UiProof,
}
#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum MotionCommand {
    Offer {
        version: u16,
        sequence: u64,
        fence: MotionFence,
        activity: &'static str,
        from: CssPoint,
        target: CssPoint,
        maximum_speed: f64,
        acceleration: f64,
        valid_for_ms: u64,
    },
    Pause {
        version: u16,
        sequence: u64,
        fence: MotionFence,
    },
    Resume {
        version: u16,
        sequence: u64,
        fence: MotionFence,
    },
    Terminal {
        version: u16,
        sequence: u64,
        fence: MotionFence,
        status: crate::action_executor::Status,
    },
}
