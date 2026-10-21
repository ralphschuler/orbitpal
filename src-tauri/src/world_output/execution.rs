//! Per-job measured evidence. These values never deserialize as authority.
use super::*;
use crate::world_state::coordinates::Point;
use std::collections::VecDeque;
#[derive(Clone)]
pub(crate) struct VisualExecution {
    pub output: OutputProof,
    pub activity: crate::life_domain::Activity,
    pub minimum_active_ms: u64,
    pub target: Point,
    pub from: CssPoint,
    pub destination: CssPoint,
    pub maximum_speed: f64,
    pub acceleration: f64,
    pub fence: Option<MotionFence>,
    pub last_receipt: u64,
    pub last_hash: Option<[u8; 32]>,
    pub last_frame: u64,
    pub last_anchor: CssPoint,
    pub last_observed: u64,
    pub proof_received: Option<u64>,
    pub started_at: Option<u64>,
    pub highest_progress: f64,
    pub stable: VecDeque<(u64, u64)>,
}
impl VisualExecution {
    pub(crate) fn new(output: OutputProof, target: Point, now: u64) -> Result<Self, RenderReason> {
        Self::for_activity(output, target, crate::life_domain::Activity::MoveTo, 0, now)
    }
    pub(crate) fn for_activity(
        output: OutputProof,
        target: Point,
        activity: crate::life_domain::Activity,
        minimum_active_ms: u64,
        now: u64,
    ) -> Result<Self, RenderReason> {
        use crate::life_domain::Activity;
        let station = matches!(activity, Activity::Rest | Activity::Recharge | Activity::Snack
            | Activity::Play | Activity::Socialize);
        if minimum_active_ms > crate::life_self_care::MAX_CARE_DURATION_MS
            || activity == Activity::MoveTo && minimum_active_ms != 0
            || station && (!output.binding.station_supported || minimum_active_ms == 0)
            || activity != Activity::MoveTo && !station
        {
            return Err(RenderReason::UnsupportedCapability);
        }
        let from = output.binding.position;
        let destination = css(target, output.binding.viewport);
        if !output.binding.safe_centers.contains(from)
            || !output.binding.safe_centers.contains(destination)
        {
            return Err(RenderReason::TargetDoesNotFit);
        }
        Ok(Self {
            output,
            activity,
            minimum_active_ms,
            target,
            from,
            destination,
            maximum_speed: 120.0,
            acceleration: 360.0,
            fence: None,
            last_receipt: 0,
            last_hash: None,
            last_frame: 0,
            last_anchor: from,
            last_observed: now,
            proof_received: None,
            started_at: None,
            highest_progress: 0.0,
            stable: VecDeque::new(),
        })
    }
    pub(crate) fn fence(
        &self,
        action: crate::life_domain::ActionId,
        delivery: String,
    ) -> MotionFence {
        let b = &self.output.binding;
        MotionFence {
            actor_id: b.actor_id.clone(),
            render_session_id: b.render_session_id.clone(),
            binding_sequence: b.binding_sequence,
            world_instance_id: b.world_instance_id.clone(),
            stage_id: b.stage_id.clone(),
            config_revision: b.config_revision,
            topology_revision: b.topology_revision,
            layout_revision: b.layout_revision,
            asset_revision: b.asset_revision,
            measurement_revision: b.measurement_revision,
            reference_revision: b.reference_revision,
            action_id: action,
            delivery_id: delivery,
        }
    }
    pub(crate) fn current(&self, registry: &Registry, now: u64) -> bool {
        registry.check_proof(&self.output, now)
    }
    pub(crate) fn fresh(&self, now: u64) -> bool {
        self.proof_received
            .is_some_and(|t| now >= t && now - t <= FRAME_MS)
    }
    pub(crate) fn observe(
        &mut self,
        frame: &FrameObservation,
        kind: ReportKind,
        received: u64,
        now: u64,
    ) -> Result<Point, RenderReason> {
        if received > now
            || now - received > FRAME_MS
            || received < self.last_observed
            || frame.frame_sequence <= self.last_frame
        {
            return Err(RenderReason::Expired);
        }
        if matches!(kind, ReportKind::Delivered | ReportKind::Started)
            && frame.anchor.distance(self.from) > 0.25
        {
            return Err(RenderReason::ProtocolError);
        }
        let dx = self.destination.x - self.from.x;
        let dy = self.destination.y - self.from.y;
        let len = dx * dx + dy * dy;
        let t = if len > 1e-12 {
            ((frame.anchor.x - self.from.x) * dx + (frame.anchor.y - self.from.y) * dy) / len
        } else {
            0.0
        };
        let longitudinal = t * len.sqrt();
        if kind == ReportKind::Started {
            self.started_at = Some(received);
        }
        let travel_limit = if let Some(started) = self.started_at {
            let seconds = received.saturating_sub(started) as f64 / 1000.0;
            let ramp = self.maximum_speed / self.acceleration;
            if seconds <= ramp {
                0.5 * self.acceleration * seconds * seconds
            } else {
                self.maximum_speed * seconds - 0.5 * self.maximum_speed * ramp
            }
        } else {
            0.0
        };
        let projected = CssPoint {
            x: self.from.x + dx * t.clamp(0.0, 1.0),
            y: self.from.y + dy * t.clamp(0.0, 1.0),
        };
        if longitudinal + 0.25 < self.highest_progress
            || frame.anchor.distance(self.from) > travel_limit + 0.5
            || (!frame.walking && frame.anchor.distance(self.last_anchor) > 0.25)
            || longitudinal < -0.25
            || longitudinal > len.sqrt() + 0.25
            || projected.distance(frame.anchor) > 0.5
            || frame.anchor.distance(self.last_anchor)
                > self.maximum_speed * (received - self.last_observed) as f64 / 1000.0 + 0.5
        {
            return Err(RenderReason::ProtocolError);
        }
        if kind != ReportKind::Delivered
            && !frame.walking
            && frame.anchor.distance(self.destination) <= 0.25
        {
            if self
                .stable
                .back()
                .is_some_and(|(_, t)| received < t.saturating_add(1) || received - t > FRAME_MS)
            {
                self.stable.clear();
            }
            if self.stable.len() >= 3 {
                // Retain the first stable observation plus the latest two.
                // A faster report cadence must not erase the required100ms span.
                self.stable.remove(1);
            }
            self.stable.push_back((frame.frame_sequence, received));
        } else {
            self.stable.clear();
        }
        self.highest_progress = self.highest_progress.max(longitudinal);
        self.last_frame = frame.frame_sequence;
        self.last_observed = received;
        self.last_anchor = frame.anchor;
        self.proof_received = Some(received);
        if kind == ReportKind::Completed
            && (self.stable.len() < 3
                || self.stable.front().is_none_or(|(_, t)|
                    received - t < self.minimum_active_ms.max(100)))
        {
            return Err(RenderReason::MeasurementUnavailable);
        }
        canonical(frame.anchor, self.output.binding.viewport)
    }
}
