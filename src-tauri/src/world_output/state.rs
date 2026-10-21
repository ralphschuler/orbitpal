//! Core-owned, bounded observations; neither measurement nor registration mints
//! Life/Store authority. A mutable payload never supplies native revisions.
use super::wire::*;
use crate::{
    action_executor::OutputStamp,
    life_runtime::action_state::WorldActionFence,
    storage::action_store::ProfileToken,
    world_state::coordinates::{Point, Rect, SCALE},
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
/// Host-elected transport. Native identity is never deserialized from IPC.
#[derive(Clone, Default)]
pub(crate) enum SourceRoute {
    #[default]
    Studio,
    Native(crate::life_runtime::native_scene::NativeRouteProof),
}
impl SourceRoute {
    fn same(&self, other:&Self) -> bool {
        match (self,other) {
            (Self::Studio,Self::Studio) => true,
            (Self::Native(a),Self::Native(b)) => a.same_route(b),
            _ => false,
        }
    }
    fn calibration_current(&self, now:u64) -> bool {
        match self { Self::Studio => true, Self::Native(p) => p.preparing_current(now) || p.live_current(now) }
    }
    pub(crate) fn live_current(&self, now:u64) -> bool {
        match self { Self::Studio => true, Self::Native(p) => p.live_current(now) }
    }
}
#[derive(Clone)]
pub(crate) struct NativeSource {
    pub route: SourceRoute,
    pub context: WorldActionFence,
    pub profile_token: ProfileToken,
    pub ui_connection: String,
    pub layout_token: Value,
    pub viewport_limit: CssSize,
    pub safe_region: Rect,
    pub profile_scale: f64,
    pub effective_scale: f64,
    pub native_walk: bool,
    pub owner: bool,
    pub native_framing: Option<NativePhysicalPolicy>,
}
impl NativeSource {
    fn same_binding(&self, other: &Self) -> bool {
        self.route.same(&other.route) && self.context.session == other.context.session
            && self.context.source_world == other.context.source_world
            && self.context.stage == other.context.stage
            && self.context.instance_id == other.context.instance_id
            && self.context.config_revision == other.context.config_revision
            && self.context.topology_revision == other.context.topology_revision
            && self.context.record == other.context.record
            && self.profile_token == other.profile_token
            && self.ui_connection == other.ui_connection
            && self.layout_token == other.layout_token
            && self.viewport_limit == other.viewport_limit
            && self.profile_scale == other.profile_scale
            && self.effective_scale == other.effective_scale
            && self.native_walk == other.native_walk
            && self.owner == other.owner
            && self.native_framing == other.native_framing
    }
}
#[derive(Clone)]
struct Pending {
    challenge: MeasurementChallenge,
    issued: u64,
}
#[derive(Clone)]
struct Attachment {
    mount: String,
    source: NativeSource,
    stamp: BindingStamp,
    surface: String,
    cancel: Arc<AtomicBool>,
    pending: Option<Pending>,
    binding: Option<MotionBinding>,
    clip: Option<CssRect>,
    frame: u64,
    observed: Option<u64>,
}
#[derive(Clone)]
pub(crate) struct OutputProof {
    pub route: SourceRoute,
    pub stamp: OutputStamp,
    pub binding: MotionBinding,
    pub cancel: Arc<AtomicBool>,
    pub profile_token: ProfileToken,
    pub ui_connection: String,
}
#[derive(Default)]
pub(crate) struct Registry {
    entries: BTreeMap<String, Attachment>,
    surfaces: BTreeMap<String, String>,
    next_revision: u64,
    outbox: VecDeque<MotionCommand>,
    terminals: VecDeque<(
        MotionFence,
        crate::action_executor::Status,
        u64,
        Option<[u8; 32]>,
    )>,
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-8
}
fn same_ground(a: GroundBasis, b: GroundBasis) -> bool {
    near(a.right.x, b.right.x)
        && near(a.right.z, b.right.z)
        && near(a.down.x, b.down.x)
        && near(a.down.z, b.down.z)
}
pub(crate) fn css(point: Point, viewport: CssSize) -> CssPoint {
    CssPoint {
        x: f64::from(point.x()) / f64::from(SCALE) * viewport.width,
        y: f64::from(point.y()) / f64::from(SCALE) * viewport.height,
    }
}
pub(crate) fn canonical(point: CssPoint, viewport: CssSize) -> Result<Point, RenderReason> {
    if !viewport.valid()
        || !point.valid()
        || point.x < 0.0
        || point.y < 0.0
        || point.x > viewport.width
        || point.y > viewport.height
    {
        return Err(RenderReason::ProtocolError);
    }
    Point::new(
        (point.x / viewport.width * f64::from(SCALE)).round() as u32,
        (point.y / viewport.height * f64::from(SCALE)).round() as u32,
    )
    .map_err(|_| RenderReason::ProtocolError)
}
/// Non-authorizing camera math: callers still supply and validate a genuine
/// source, new submitted frames and the frozen all-pose measured enclosure.
pub(super) fn measured_safe_centers(safe:Rect,clip:CssRect,viewport:CssSize,envelope:EnvelopeExtents)->CssRect {
    CssRect {
        left:(f64::from(safe.min().x())/f64::from(SCALE)*viewport.width).max(clip.left)+envelope.left,
        top:(f64::from(safe.min().y())/f64::from(SCALE)*viewport.height).max(clip.top)+envelope.top,
        right:(f64::from(safe.max().x())/f64::from(SCALE)*viewport.width).min(clip.right)-envelope.right,
        bottom:(f64::from(safe.max().y())/f64::from(SCALE)*viewport.height).min(clip.bottom)-envelope.bottom,
    }
}
#[path = "semantic.rs"]
mod semantic;
#[path = "dispatch.rs"]
mod dispatch;
pub(crate) use dispatch::RoutedWorldCommand;

impl Registry {
    fn revision(&mut self) -> Result<u64, RenderReason> {
        self.next_revision = self
            .next_revision
            .checked_add(1)
            .filter(|n| counter(*n))
            .ok_or(RenderReason::Capacity)?;
        Ok(self.next_revision)
    }
    fn challenge(entry: &mut Attachment, now: u64) {
        if entry
            .pending
            .as_ref()
            .is_some_and(|p| now >= p.issued && now - p.issued < TTL_MS)
        {
            return;
        }
        entry.pending = Some(Pending {
            issued: now,
            challenge: MeasurementChallenge {
                id: uuid::Uuid::new_v4().to_string(),
                expires_in_ms: TTL_MS as u16,
                expected: entry.stamp.clone(),
                camera_policy: if entry.source.native_framing.is_some() { CameraPolicy::NativePhysicalIsoV1 } else { CameraPolicy::WorldIsoV1 },
                native_framing: entry.source.native_framing,
                enclosure_kind: EnclosureKind::ConservativeSubmittedGeometryV1,
                profile_scale: entry.source.profile_scale,
                effective_scale: entry.source.effective_scale,
                expected_reference_local: entry.binding.as_ref().map(|b| b.reference_local)
                    .or_else(|| entry.source.native_framing.map(|_| LocalReference { x:0.0, y:1.45, z:0.0 })),
            },
        });
    }
    fn reply(entry: &Attachment, challenge: bool, now: u64) -> RegisteredWorldActor {
        let readiness = if entry.cancel.load(Ordering::SeqCst) {
            Readiness::Unavailable {
                reason: RenderReason::Cancelled,
            }
        } else if !entry.source.route.calibration_current(now) {
            Readiness::Unavailable {
                reason: RenderReason::Cancelled,
            }
        } else if challenge {
            entry
                .pending
                .as_ref()
                .map(|p| Readiness::AwaitingMeasurement {
                    challenge: p.challenge.clone(),
                })
                .unwrap_or(Readiness::Unavailable {
                    reason: RenderReason::Expired,
                })
        } else if let Some(binding) = entry.binding.as_ref().filter(|_| {
            entry.source.route.live_current(now) && entry
                .observed
                .is_some_and(|t| now >= t && now - t <= TTL_MS)
        }) {
            Readiness::Ready {
                binding: binding.clone(),
            }
        } else {
            Readiness::Unavailable {
                reason: RenderReason::Expired,
            }
        };
        RegisteredWorldActor {
            version: 1,
            render_session_id: entry.stamp.render_session_id.clone(),
            binding_sequence: entry.stamp.binding_sequence,
            surface_id: entry.surface.clone(),
            role: if entry.source.owner {
                RenderRole::OwnerCandidate
            } else {
                RenderRole::Mirror
            },
            readiness,
        }
    }
    pub(crate) fn register(
        &mut self,
        request: &RegisterWorldActor,
        source: NativeSource,
        now: u64,
    ) -> Result<(RegisteredWorldActor, Arc<AtomicBool>, bool), RenderReason> {
        let valid_request = match &source.route {
            SourceRoute::Studio => request.valid(),
            SourceRoute::Native(_) => request.valid_native_scope() && request.studio_proof.is_none(),
        };
        if !valid_request
            || !source.route.calibration_current(now)
            || matches!(&source.route, SourceRoute::Native(_)) != source.native_framing.is_some()
            || !counter(now.saturating_add(1))
            || request.actor_id != source.context.session.actor_id().as_str()
            || request.world_instance_id != source.context.instance_id
            || request.stage_id != source.context.stage.as_str()
            || request.config_revision != source.context.config_revision
            || ![source.profile_scale, source.effective_scale]
                .into_iter()
                .all(|s| s.is_finite() && (0.3..=2.5).contains(&s))
            || source.native_framing.is_some_and(|policy| !policy.valid()
                || !near(policy.viewport().width, source.viewport_limit.width)
                || !near(policy.viewport().height, source.viewport_limit.height))
        {
            return Err(RenderReason::ScopeMismatch);
        }
        // A Studio repaint cannot replace a physically retained native owner,
        // including revoked/expired work still awaiting exact cleanup. Its
        // read-only semantic mirror is a separate projection.
        if self.entries.get(&request.actor_id).is_some_and(|e|
            matches!(source.route, SourceRoute::Studio)
                && matches!(&e.source.route, SourceRoute::Native(route) if route.retained())) {
            return Err(RenderReason::ScopeMismatch);
        }
        if let Some(entry) = self.entries.get_mut(&request.actor_id) {
            if entry.mount == request.mount_id
                && entry.source.same_binding(&source)
                && !entry.cancel.load(Ordering::SeqCst)
            {
                entry.source.context.position = source.context.position;
                if let Some(b) = &mut entry.binding {
                    b.position = css(source.context.position, b.viewport);
                }
                Self::challenge(entry, now);
                return Ok((Self::reply(entry, true, now), entry.cancel.clone(), false));
            }
        }
        self.entries.retain(|_, entry| !entry.cancel.load(Ordering::SeqCst)
            || matches!(&entry.source.route, SourceRoute::Native(route) if route.retained()));
        if !self.entries.contains_key(&request.actor_id) && self.entries.len() >= MAX_LAYERS {
            return Err(RenderReason::Capacity);
        }
        let revision = self.revision()?;
        let namespace = match &source.route { SourceRoute::Studio => format!("studio:{}",source.ui_connection),
            SourceRoute::Native(p) => format!("native:{}",p.namespace()) };
        // Only namespaces with a retained attachment survive. This stays
        // bounded by MAX_LAYERS rather than renderer-controlled mount strings.
        self.surfaces.retain(|_,id| self.entries.values().any(|e| &e.surface == id));
        if !self.surfaces.contains_key(&namespace) && self.surfaces.len() >= MAX_LAYERS {
            return Err(RenderReason::Capacity);
        }
        let surface = self.surfaces.entry(namespace).or_insert_with(||uuid::Uuid::new_v4().to_string()).clone();
        if let Some(old) = self.entries.remove(&request.actor_id) {
            old.cancel.store(true, Ordering::SeqCst);
        }
        let render = uuid::Uuid::new_v4().to_string();
        let stamp = BindingStamp {
            actor_id: request.actor_id.clone(),
            source_world_id: source.context.source_world.as_str().into(),
            render_session_id: render,
            binding_sequence: revision,
            world_instance_id: source.context.instance_id.clone(),
            stage_id: source.context.stage.as_str().into(),
            config_revision: source.context.config_revision,
            topology_revision: source.context.topology_revision,
            layout_revision: revision,
            asset_revision: revision,
            measurement_revision: revision,
            reference_revision: revision,
        };
        let mut entry = Attachment {
            mount: request.mount_id.clone(),
            source,
            stamp,
            surface,
            cancel: Arc::new(AtomicBool::new(false)),
            pending: None,
            binding: None,
            clip: None,
            frame: 0,
            observed: None,
        };
        Self::challenge(&mut entry, now);
        let reply = Self::reply(&entry, true, now);
        let cancel = entry.cancel.clone();
        self.entries.insert(request.actor_id.clone(), entry);
        Ok((reply, cancel, true))
    }
    pub(crate) fn measurement_actor(&self, id: &str) -> Option<String> {
        self.entries.iter().find_map(|(bot, e)| {
            e.pending
                .as_ref()
                .filter(|p| p.challenge.id == id)
                .map(|_| bot.clone())
        })
    }
    pub(crate) fn source(&self, bot: &str) -> Option<NativeSource> {
        self.entries.get(bot).map(|e| e.source.clone())
    }
    pub(crate) fn matches(&self, bot: &str, source: &NativeSource) -> bool {
        self.entries
            .get(bot)
            .is_some_and(|e| !e.cancel.load(Ordering::SeqCst) && e.source.same_binding(source))
    }
    pub(crate) fn measurement(
        &mut self,
        report: &RenderMeasurement,
        source: &NativeSource,
        received: u64,
        now: u64,
    ) -> Result<MeasurementAck, RenderReason> {
        let bot = self
            .measurement_actor(&report.challenge_id)
            .ok_or(RenderReason::StaleBinding)?;
        let entry = self
            .entries
            .get_mut(&bot)
            .ok_or(RenderReason::Unregistered)?;
        let pending = entry.pending.as_ref().ok_or(RenderReason::StaleBinding)?;
        if received > now
            || !source.route.calibration_current(now)
            || now - received > FRAME_MS
            || !report.valid()
            || report.enclosure_kind != pending.challenge.enclosure_kind
            // The host-prescribed first reference must match before binding.
            // Renewals reach the frozen binding comparison below, which also
            // revokes an existing binding when its reference changes.
            || (entry.binding.is_none() && pending.challenge.expected_reference_local.is_some_and(|reference| reference != report.reference_local))
            || !entry.source.same_binding(source)
            || entry.cancel.load(Ordering::SeqCst)
            || now < pending.issued
            || now - pending.issued >= TTL_MS
            || report.frame_sequence <= entry.frame
        {
            return Err(RenderReason::StaleBinding);
        }
        let full = CssRect {
            left: 0.0,
            top: 0.0,
            right: report.viewport.width,
            bottom: report.viewport.height,
        };
        let hull = report.envelope_extents.bounds(report.anchor);
        if !entry.source.viewport_limit.valid()
            || report.viewport.width > entry.source.viewport_limit.width + 0.5
            || report.viewport.height > entry.source.viewport_limit.height + 0.5
            || !full.contains_rect(report.clip)
            || !report.clip.contains_rect(report.posed_bounds)
            || !hull.contains_rect(report.posed_bounds)
            || entry.source.native_framing.is_some_and(|policy|
                (policy.viewport().width - report.viewport.width).abs() > 0.5
                    || (policy.viewport().height - report.viewport.height).abs() > 0.5
                    || !near(report.applied_model_scale, entry.source.effective_scale))
        {
            return Err(RenderReason::MeasurementUnavailable);
        }
        // Both closed policies use elevation pi/6 and positive cameraZ. Native
        // span comes only from the host's retained normal physical reference.
        let span = entry.source.native_framing.map_or(8.0, |policy| policy.vertical_span);
        let unit = span / report.viewport.height / report.applied_model_scale;
        let expected = GroundBasis {
            right: GroundVector { x: unit, z: 0.0 },
            down: GroundVector {
                x: 0.0,
                z: unit / 0.5,
            },
        };
        if !same_ground(expected, report.ground_basis) {
            return Err(RenderReason::MeasurementUnavailable);
        }
        let radius_x = report
            .envelope_extents
            .left
            .max(report.envelope_extents.right);
        let radius_y = report
            .envelope_extents
            .top
            .max(report.envelope_extents.bottom);
        // Preserve Studio's established symmetric safe region. Native uses the
        // actual asymmetric all-pose envelope around its prescribed rig anchor.
        let safe_envelope=match &source.route { SourceRoute::Native(_)=>report.envelope_extents,
            SourceRoute::Studio=>EnvelopeExtents{left:radius_x,right:radius_x,top:radius_y,bottom:radius_y} };
        let safe_centers=measured_safe_centers(entry.source.safe_region,report.clip,report.viewport,safe_envelope);
        let position = css(source.context.position, report.viewport);
        if !safe_centers.valid()
            || !safe_centers.contains(position)
            || !safe_centers.contains(report.anchor)
            || report.anchor.distance(position) > 0.25
        {
            return Err(RenderReason::TargetDoesNotFit);
        }
        if let Some(binding) = &entry.binding {
            if binding.enclosure_kind != report.enclosure_kind
                || binding.viewport != report.viewport
                || entry.clip != Some(report.clip)
                || binding.envelope_extents != report.envelope_extents
                || binding.reference_local != report.reference_local
                || !same_ground(binding.ground_basis, report.ground_basis)
                || !near(binding.scale, report.applied_model_scale)
                || !near(binding.model_forward_yaw, report.model_forward_yaw)
                || binding.reduced_motion != report.reduced_motion
                || binding.walk_supported != (report.walk_supported && source.native_walk)
            {
                entry.cancel.store(true, Ordering::SeqCst);
                return Err(RenderReason::StaleBinding);
            }
        }
        // Normalize the closed camera basis on first acceptance, and retain all
        // structural floating-point values on renewal. Tiny inverse-projection
        // noise is tolerated but cannot accumulate into a drifting binding.
        let (ground_basis, scale, model_forward_yaw) = entry
            .binding
            .as_ref()
            .map(|b| (b.ground_basis, b.scale, b.model_forward_yaw))
            .unwrap_or((
                expected,
                report.applied_model_scale,
                report.model_forward_yaw,
            ));
        let s = &entry.stamp;
        entry.binding = Some(MotionBinding {
            actor_id: s.actor_id.clone(),
            source_world_id: s.source_world_id.clone(),
            render_session_id: s.render_session_id.clone(),
            binding_sequence: s.binding_sequence,
            world_instance_id: s.world_instance_id.clone(),
            stage_id: s.stage_id.clone(),
            config_revision: s.config_revision,
            topology_revision: s.topology_revision,
            layout_revision: s.layout_revision,
            asset_revision: s.asset_revision,
            measurement_revision: s.measurement_revision,
            reference_revision: s.reference_revision,
            enclosure_kind: pending.challenge.enclosure_kind,
            camera_policy: source.native_framing.map(|_| CameraPolicy::NativePhysicalIsoV1),
            native_framing: source.native_framing,
            viewport: report.viewport,
            safe_centers,
            maximum_footprint: CssSize {
                width: radius_x * 2.0,
                height: radius_y * 2.0,
            },
            envelope_extents: report.envelope_extents,
            reference_local: report.reference_local,
            position,
            ground_basis,
            model_forward_yaw,
            profile_scale: source.profile_scale,
            effective_scale: source.effective_scale,
            scale,
            reduced_motion: report.reduced_motion,
            walk_supported: report.walk_supported && source.native_walk,
        });
        entry.clip = Some(report.clip);
        entry.frame = report.frame_sequence;
        entry.observed = Some(received);
        entry.pending = None;
        Ok(MeasurementAck {
            version: 1,
            frame_sequence: report.frame_sequence,
            output: Self::reply(entry, false, now),
        })
    }
    pub(crate) fn ready(&self, bot: &str, now: u64) -> Result<OutputProof, RenderReason> {
        let e = self.entries.get(bot).ok_or(RenderReason::Unregistered)?;
        let b = e
            .binding
            .as_ref()
            .ok_or(RenderReason::MeasurementUnavailable)?;
        if !e.source.owner {
            return Err(RenderReason::ScopeMismatch);
        }
        if e.cancel.load(Ordering::SeqCst) || !e.source.route.live_current(now) {
            return Err(RenderReason::Cancelled);
        }
        if e.observed.is_none_or(|t| now < t || now - t > TTL_MS) {
            return Err(RenderReason::Expired);
        }
        if b.reduced_motion || !b.walk_supported {
            return Err(RenderReason::UnsupportedCapability);
        }
        Ok(OutputProof {
            route: e.source.route.clone(),
            stamp: OutputStamp::new(e.stamp.render_session_id.clone(), e.stamp.binding_sequence)
                .map_err(|_| RenderReason::ProtocolError)?,
            binding: b.clone(),
            cancel: e.cancel.clone(),
            profile_token: e.source.profile_token,
            ui_connection: e.source.ui_connection.clone(),
        })
    }
    pub(crate) fn check_proof(&self, proof: &OutputProof, now: u64) -> bool {
        self.ready(&proof.binding.actor_id, now).is_ok_and(|p| {
            p.stamp == proof.stamp
                && {
                    let mut actual = p.binding.clone();
                    actual.position = proof.binding.position;
                    actual == proof.binding
                }
                && Arc::ptr_eq(&p.cancel, &proof.cancel)
        })
    }
    pub(crate) fn check_frame(
        &self,
        proof: &OutputProof,
        frame: &FrameObservation,
        now: u64,
    ) -> Result<(), RenderReason> {
        self.ready(&proof.binding.actor_id, now)?;
        let e = self
            .entries
            .get(&proof.binding.actor_id)
            .ok_or(RenderReason::Unregistered)?;
        let b = e.binding.as_ref().ok_or(RenderReason::Unregistered)?;
        if e.stamp.render_session_id != proof.binding.render_session_id
            || e.stamp.binding_sequence != proof.binding.binding_sequence
            || !frame.valid()
            || frame.enclosure_kind != b.enclosure_kind
            || frame.frame_sequence <= e.frame
            || frame.viewport != b.viewport
            || Some(frame.clip) != e.clip
            || !b.safe_centers.contains(frame.anchor)
            || !frame.clip.contains_rect(frame.posed_bounds)
            || !b
                .envelope_extents
                .bounds(frame.anchor)
                .contains_rect(frame.posed_bounds)
        {
            return Err(RenderReason::MeasurementUnavailable);
        }
        Ok(())
    }
    pub(crate) fn accept_frame(
        &mut self,
        proof: &OutputProof,
        frame: &FrameObservation,
        position: Point,
        now: u64,
    ) -> Result<(), RenderReason> {
        self.check_frame(proof, frame, now)?;
        let e = self
            .entries
            .get_mut(&proof.binding.actor_id)
            .ok_or(RenderReason::Unregistered)?;
        e.frame = frame.frame_sequence;
        e.observed = Some(now);
        e.source.context.position = position;
        if let Some(b) = &mut e.binding {
            b.position = css(position, b.viewport);
        }
        Ok(())
    }
    pub(crate) fn cancel_actor(&mut self, bot: Option<&str>) {
        for (id, e) in &self.entries {
            if bot.is_none_or(|b| b == id) {
                e.cancel.store(true, Ordering::SeqCst);
            }
        }
    }
    pub(crate) fn unregister(&mut self, request: &UnregisterWorldActor, ui: &str) -> bool {
        let key = self.entries.iter().find_map(|(bot, e)| {
            (request.version == 1
                && e.stamp.render_session_id == request.render_session_id
                && e.stamp.binding_sequence == request.binding_sequence
                && e.source.ui_connection == ui)
                .then(|| bot.clone())
        });
        if let Some(key) = key {
            if let Some(e) = self.entries.get(&key) {
                e.cancel.store(true, Ordering::SeqCst);
                // Revocation is immediate, but a native physical cleanup lease
                // must remain visible to replacement/pruning barriers until its
                // exact member acknowledges actual restoration or retirement.
                if matches!(&e.source.route, SourceRoute::Native(route) if route.retained()) {
                    return true;
                }
            }
            self.entries.remove(&key);
            true
        } else {
            false
        }
    }
    pub(crate) fn status(&self, bot: &str, now: u64) -> Option<RegisteredWorldActor> {
        self.entries.get(bot).map(|e| Self::reply(e, false, now))
    }
    pub(crate) fn record_terminal(
        &mut self,
        fence: MotionFence,
        status: crate::action_executor::Status,
        sequence: u64,
        hash: Option<[u8; 32]>,
    ) {
        if self.terminals.len() >= MAX_OUTBOX {
            self.terminals.pop_front();
        }
        self.terminals.push_back((fence, status, sequence, hash));
    }
    pub(crate) fn terminal_duplicate(
        &self,
        fence: &MotionFence,
        sequence: u64,
        hash: &[u8; 32],
    ) -> Option<crate::action_executor::Status> {
        self.terminals
            .iter()
            .rev()
            .find(|(f, _, s, h)| f == fence && *s == sequence && h.as_ref() == Some(hash))
            .map(|(_, s, _, _)| *s)
    }
    pub(crate) fn enqueue(&mut self, command: MotionCommand) -> Result<(), RenderReason> {
        if self.outbox.len() >= MAX_OUTBOX {
            return Err(RenderReason::Capacity);
        }
        self.outbox.push_back(command);
        Ok(())
    }
    pub(crate) fn take_command(&mut self) -> Option<MotionCommand> {
        self.outbox.pop_front()
    }
}
