//! Pure bounded executor foundation. NOT registered with Tauri/Core/Store yet.
//! Every authority/commit argument must be supplied by the future native host
//! after actual Core/WorldHub/Store checks, never deserialized from a receipt.
#[path = "action_executor/catalog.rs"]
mod catalog;
use crate::{
    life_domain::{
        self, ActionId, ActionIntent, ActionPhase, Activity, ActorId, CancelReason, CommittedEvent,
        LifeEvent, LifeState, RandomState, WorldId, MAX_COUNTER,
    },
    world_state::{coordinates::Point, AnchorKind, SessionStamp, SlotId, SlotLease, StageId},
};
use catalog::Policy;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) const MAX_JOBS: usize = 64;
pub(crate) const MAX_OUTBOX: usize = 256;
pub(crate) const RECEIPT_TIMEOUT_MS: u64 = 5000;
pub(crate) const MAX_PAUSE_MS: u64 = 5000;
const MAX_PROGRESS_GAP_MS: u64 = 5000;
const ARRIVAL_TOLERANCE: i64 = 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionError {
    InvalidId,
    InvalidClock,
    InvalidGoal,
    InvalidState,
    Busy,
    Capacity,
    UnsupportedCapability,
    WrongAuthority,
    StaleReceipt,
    InvalidTransition,
    ResourceRequired,
    InvalidCommit,
    Exhausted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Presentation {
    Visual,
    NativeRestDwell,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Goal {
    Here(Point),
    Point(Point),
    Slot {
        id: SlotId,
        kind: AnchorKind,
        position: Point,
    },
    Actor {
        id: ActorId,
        position: Point,
    },
}
impl Goal {
    fn position(&self) -> Point {
        match self {
            Self::Here(p) | Self::Point(p) => *p,
            Self::Slot { position, .. } | Self::Actor { position, .. } => *position,
        }
    }
    fn needs_slot(&self) -> bool {
        matches!(self, Self::Slot { .. })
    }
}
fn valid_id(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn counter(value: u64) -> Result<u64, ActionError> {
    if (1..=MAX_COUNTER).contains(&value) {
        Ok(value)
    } else {
        Err(ActionError::InvalidState)
    }
}

/// Native-only output registration fence. No Deserialize/Serialize implementation.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct OutputStamp {
    connection: String,
    generation: u64,
}
impl OutputStamp {
    pub fn new(connection: impl Into<String>, generation: u64) -> Result<Self, ActionError> {
        let connection = connection.into();
        if !valid_id(&connection, 96) {
            return Err(ActionError::InvalidId);
        }
        Ok(Self {
            connection,
            generation: counter(generation)?,
        })
    }
}
/// Correlation of a real current native session with a selected output/asset.
/// Constructing this value is NOT a Core check. The integrating host MUST check
/// actual Mode::Life, session, system/UI/output and WorldHub state each time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeAuthority {
    session: SessionStamp,
    source_world: WorldId,
    stage: StageId,
    config_revision: u64,
    topology_revision: u64,
    output: OutputStamp,
    asset_revision: u64,
}
impl NativeAuthority {
    pub fn new(
        session: SessionStamp,
        source_world: WorldId,
        stage: StageId,
        config_revision: u64,
        topology_revision: u64,
        output: OutputStamp,
        asset_revision: u64,
    ) -> Result<Self, ActionError> {
        Ok(Self {
            session,
            source_world,
            stage,
            config_revision: counter(config_revision)?,
            topology_revision: counter(topology_revision)?,
            output,
            asset_revision: counter(asset_revision)?,
        })
    }
    pub fn actor(&self) -> &ActorId {
        self.session.actor_id()
    }
}
/// Internal, intersected native-asset + loaded-output capability observation.
/// This is not an IPC capability grant and is not persisted/exported.
#[derive(Clone, Debug)]
pub(crate) struct Capabilities {
    authority: NativeAuthority,
    visual: Vec<(Activity, u64)>,
    native_rest_dwell_opt_in: bool,
}
impl Capabilities {
    pub fn new(
        authority: NativeAuthority,
        visual: Vec<(Activity, u64)>,
        native_rest_dwell_opt_in: bool,
    ) -> Result<Self, ActionError> {
        if visual.len() > 6
            || visual.iter().any(|(_, ms)| *ms > 60_000)
            || visual.iter().enumerate().any(|(index, (kind, _))| {
                visual[..index].iter().any(|(previous, _)| previous == kind)
            })
        {
            return Err(ActionError::UnsupportedCapability);
        }
        Ok(Self {
            authority,
            visual,
            native_rest_dwell_opt_in,
        })
    }
}

/// Prepared native Plan only. It cannot apply a reward or dispatch presentation.
/// The private exact expected intent/state prevents changing the catalog effect
/// between Plan construction and the real Store receipt.
#[derive(Clone)]
pub(crate) struct PreparedPlan {
    authority: NativeAuthority,
    goal: Goal,
    policy: Policy,
    before_revision: u64,
    event: LifeEvent,
    expected: LifeState,
    intent: ActionIntent,
}
impl PreparedPlan {
    pub(crate) fn visual_minimum(activity: Activity, goal: &Goal) -> Result<u64, ActionError> {
        Ok(Policy::for_request(activity, goal, Presentation::Visual)?.minimum_active_ms)
    }
    pub fn new(
        authority: NativeAuthority,
        before: &LifeState,
        random: RandomState,
        activity: Activity,
        goal: Goal,
        presentation: Presentation,
        capabilities: &Capabilities,
    ) -> Result<Self, ActionError> {
        Self::configured(authority, before, random, activity, goal, presentation, capabilities, None)
    }
    pub(crate) fn care(
        authority: NativeAuthority,
        before: &LifeState,
        random: RandomState,
        activity: Activity,
        goal: Goal,
        presentation: Presentation,
        capabilities: &Capabilities,
        config: &crate::life_self_care::CareConfig,
    ) -> Result<Self, ActionError> {
        Self::configured(authority, before, random, activity, goal, presentation, capabilities, Some(config))
    }
    fn configured(
        authority: NativeAuthority,
        before: &LifeState,
        random: RandomState,
        activity: Activity,
        goal: Goal,
        presentation: Presentation,
        capabilities: &Capabilities,
        care: Option<&crate::life_self_care::CareConfig>,
    ) -> Result<Self, ActionError> {
        before.validate().map_err(|_| ActionError::InvalidState)?;
        if before.actor_id() != authority.actor()
            || before.world_id() != &authority.source_world
            || capabilities.authority != authority
        {
            return Err(ActionError::WrongAuthority);
        }
        let mut policy = Policy::for_request(activity, &goal, presentation)?;
        if let Some(config) = care {
            if presentation != Presentation::Visual { return Err(ActionError::UnsupportedCapability); }
            policy.effect = catalog::Effect::care(activity, &goal, config)?;
        }
        match presentation {
            Presentation::Visual => {
                let minimum = capabilities
                    .visual
                    .iter()
                    .find(|(kind, _)| *kind == activity)
                    .map(|(_, minimum)| *minimum)
                    .ok_or(ActionError::UnsupportedCapability)?;
                policy.minimum_active_ms = policy.minimum_active_ms.max(minimum);
                if care.is_some() { policy.maximum_run_ms=policy.maximum_run_ms.max(policy.minimum_active_ms.saturating_add(15_000)); }
                if policy.minimum_active_ms >= policy.maximum_run_ms {
                    return Err(ActionError::UnsupportedCapability);
                }
            }
            Presentation::NativeRestDwell if capabilities.native_rest_dwell_opt_in => (),
            Presentation::NativeRestDwell => return Err(ActionError::UnsupportedCapability),
        }
        let event = LifeEvent::Plan {
            activity,
            completion_effect: policy.effect.event_value(),
        };
        // Plan simulation allocates the expected identity/policy only. It never
        // simulates Complete or mutates the supplied/actual persistent state.
        let transition = life_domain::reduce(before, 0, random, event.clone())
            .map_err(|_| ActionError::InvalidState)?;
        let intent = transition
            .intents
            .first()
            .filter(|_| transition.intents.len() == 1)
            .cloned()
            .ok_or(ActionError::InvalidState)?;
        Ok(Self {
            authority,
            goal,
            policy,
            before_revision: before.revision(),
            event,
            expected: transition.state,
            intent,
        })
    }
    pub fn event(&self) -> &LifeEvent {
        &self.event
    }
    pub fn expected_revision(&self) -> u64 {
        self.before_revision
    }
    pub fn action_id(&self) -> &ActionId {
        self.intent.id()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    AwaitingDelivery,
    Delivered,
    Started,
    Paused,
    Completed,
    Cancelled,
    Failed,
}
impl Status {
    fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Failure {
    Unsupported,
    OutputLost,
    Timeout,
    Interrupted,
    Storage,
    Protocol,
    Exhausted,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct JobKey {
    actor: ActorId,
    action: ActionId,
    run: String,
    serial: u64,
}
impl JobKey {
    pub fn action_id(&self) -> &ActionId {
        &self.action
    }
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Evidence {
    Arrived(Point),
    VisualFinished,
    OrientationSettled,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Receipt {
    Delivered,
    Started,
    Progress,
    Completed(Evidence),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Decision {
    Accepted,
    Duplicate,
    Commit(CommitRequest),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitRequest {
    key: JobKey,
    serial: u64,
    proposed_at: u64,
    before: LifeState,
    event: LifeEvent,
}
impl CommitRequest {
    pub fn action_id(&self) -> &ActionId {
        &self.key.action
    }
}
/// One-use native pre-write admission. Intentionally not Clone or serde. The
/// actual host holds Core, rechecks World/output authority, obtains this permit,
/// and immediately executes the synchronous Store transaction without awaiting,
/// emitting, or calling a GUI/provider in between. Only this type exposes the
/// event/revision needed for that write. It is not a wire receipt.
#[derive(Debug)]
pub(crate) struct CommitPermit {
    request: CommitRequest,
    authority: NativeAuthority,
    serial: u64,
    admitted_at: u64,
}
impl CommitPermit {
    pub fn event(&self) -> &LifeEvent {
        &self.request.event
    }
    pub fn expected_revision(&self) -> u64 {
        self.request.before.revision()
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobSnapshot {
    pub version: u16,
    pub action_id: ActionId,
    pub activity: Activity,
    pub status: Status,
    pub failure: Option<Failure>,
    pub presentation: Presentation,
    pub active_ms: u64,
    pub terminal_committed: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Message {
    Offer {
        delivery_id: String,
        action_id: ActionId,
        activity: Activity,
        presentation: Presentation,
        goal: Goal,
    },
    Pause {
        delivery_id: String,
        action_id: ActionId,
    },
    Resume {
        delivery_id: String,
        action_id: ActionId,
    },
    Terminal {
        delivery_id: String,
        action_id: ActionId,
        status: Status,
        failure: Option<Failure>,
    },
}
/// Outer native route is deliberately not serialized with public message data.
#[derive(Clone, Debug)]
pub(crate) struct Dispatch {
    pub output: OutputStamp,
    pub sequence: u64,
    pub message: Message,
    key: JobKey,
}
#[derive(Clone, Debug)]
pub(crate) struct Cleanup {
    pub key: JobKey,
    pub lease: Option<SlotLease>,
    pub cancel_reason: Option<CancelReason>,
}
#[derive(Clone)]
struct Job {
    key: JobKey,
    authority: NativeAuthority,
    intent: ActionIntent,
    goal: Goal,
    policy: Policy,
    status: Status,
    failure: Option<Failure>,
    delivery: String,
    lease: Option<SlotLease>,
    pending: Option<CommitRequest>,
    permit_serial: Option<u64>,
    rebase_count: u8,
    terminal_committed: bool,
    observed_revision: u64,
    last_receipt_sequence: u64,
    delivery_deadline: u64,
    run_deadline: u64,
    last_progress: Option<u64>,
    active_ms: u64,
    paused_at: Option<u64>,
}
#[derive(Clone)]
pub(crate) struct Executor {
    run: String,
    now: u64,
    next: u64,
    poisoned: bool,
    jobs: BTreeMap<String, Job>,
    outbox: VecDeque<Dispatch>,
    cleanup: VecDeque<Cleanup>,
    invalidated_outputs: BTreeSet<OutputStamp>,
}
impl Executor {
    pub fn new(run: impl Into<String>) -> Result<Self, ActionError> {
        let run = run.into();
        if !valid_id(&run, 64) {
            return Err(ActionError::InvalidId);
        }
        Ok(Self {
            run,
            now: 0,
            next: 1,
            poisoned: false,
            jobs: BTreeMap::new(),
            outbox: VecDeque::new(),
            cleanup: VecDeque::new(),
            invalidated_outputs: BTreeSet::new(),
        })
    }
    fn serial(&mut self) -> Result<u64, ActionError> {
        let value = self.next;
        self.next = self
            .next
            .checked_add(1)
            .filter(|value| *value <= MAX_COUNTER)
            .ok_or(ActionError::Exhausted)?;
        Ok(value)
    }
    fn clock(&mut self, raw: u64) -> Result<u64, ActionError> {
        if raw > MAX_COUNTER {
            self.poison();
            return Err(ActionError::InvalidClock);
        }
        if self.poisoned {
            return Err(ActionError::Exhausted);
        }
        self.now = self.now.max(raw);
        Ok(self.now)
    }
    fn transaction<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, ActionError>,
    ) -> Result<T, ActionError> {
        if self.poisoned {
            return Err(ActionError::Exhausted);
        }
        let mut next = self.clone();
        match operation(&mut next) {
            Ok(value) => {
                *self = next;
                Ok(value)
            }
            Err(ActionError::Exhausted | ActionError::InvalidClock) => {
                self.poison();
                Err(ActionError::Exhausted)
            }
            Err(error) => {
                self.now = self.now.max(next.now);
                Err(error)
            }
        }
    }
    fn poison(&mut self) {
        self.poisoned = true;
        self.outbox.clear();
        for job in self
            .jobs
            .values_mut()
            .filter(|job| !job.status.terminal() || !job.terminal_committed)
        {
            self.invalidated_outputs
                .insert(job.authority.output.clone());
            job.permit_serial = None;
            if job.status.terminal() {
                job.pending = None;
                continue;
            }
            job.status = Status::Failed;
            job.failure = Some(Failure::Exhausted);
            job.pending = None;
            job.permit_serial = None;
            self.cleanup.push_back(Cleanup {
                key: job.key.clone(),
                lease: job.lease.take(),
                cancel_reason: Some(CancelReason::Unavailable),
            });
        }
    }
    fn enqueue(&mut self, key: &JobKey, message: Message) -> Result<(), ActionError> {
        let job = self
            .jobs
            .get(key.actor.as_str())
            .filter(|job| &job.key == key)
            .ok_or(ActionError::StaleReceipt)?;
        let output = job.authority.output.clone();
        match &message {
            Message::Offer { .. } => self.outbox.retain(|item| {
                item.key.actor != key.actor || matches!(item.message, Message::Terminal { .. })
            }),
            Message::Pause { .. } | Message::Resume { .. } => self.outbox.retain(|item| {
                &item.key != key
                    || !matches!(item.message, Message::Pause { .. } | Message::Resume { .. })
            }),
            Message::Terminal { .. } => self.outbox.retain(|item| &item.key != key),
        }
        if self.outbox.len() >= MAX_OUTBOX {
            return Err(ActionError::Capacity);
        }
        let sequence = self.serial()?;
        self.outbox.push_back(Dispatch {
            output,
            sequence,
            message,
            key: key.clone(),
        });
        Ok(())
    }
    pub fn can_admit(&self, actor: &ActorId) -> bool {
        // Outstanding jobs still need terminal capacity even when their Offer
        // has already been drained. Reserve control + terminal room per job.
        let unfinished = self
            .jobs
            .values()
            .filter(|job| !job.status.terminal())
            .count();
        !self.poisoned
            && self.outbox.len() + unfinished * 2 + 3 <= MAX_OUTBOX
            && self.cleanup.len() < MAX_JOBS
            && self.jobs.get(actor.as_str()).is_none_or(|job| {
                job.status.terminal() && job.terminal_committed && job.pending.is_none()
            })
            && !self.cleanup.iter().any(|item| item.key.actor == *actor)
            && (self.jobs.contains_key(actor.as_str()) || self.jobs.len() < MAX_JOBS)
    }
    /// `confirmed` MUST be the actual synchronous native Store Plan result, not
    /// wire data. No output is offered before exact Plan/policy validation.
    pub fn install_plan(
        &mut self,
        prepared: PreparedPlan,
        confirmed: &LifeState,
        lease: Option<SlotLease>,
        raw: u64,
    ) -> Result<JobKey, ActionError> {
        self.transaction(|this| {
            let now = this.clock(raw)?;
            if !this.can_admit(prepared.authority.actor()) {
                return Err(ActionError::Busy);
            }
            if confirmed != &prepared.expected || confirmed.validate().is_err() {
                return Err(ActionError::InvalidCommit);
            }
            if prepared.goal.needs_slot() != lease.is_some() {
                return Err(ActionError::ResourceRequired);
            }
            let serial = this.serial()?;
            let key = JobKey {
                actor: prepared.authority.actor().clone(),
                action: prepared.intent.id().clone(),
                run: this.run.clone(),
                serial,
            };
            let delivery = format!("{}-{serial}", this.run);
            let deadline = now
                .checked_add(RECEIPT_TIMEOUT_MS)
                .filter(|value| *value <= MAX_COUNTER)
                .ok_or(ActionError::Exhausted)?;
            let message = Message::Offer {
                delivery_id: delivery.clone(),
                action_id: key.action.clone(),
                activity: prepared.policy.activity,
                presentation: prepared.policy.presentation,
                goal: prepared.goal.clone(),
            };
            this.jobs.insert(
                key.actor.as_str().to_owned(),
                Job {
                    key: key.clone(),
                    authority: prepared.authority,
                    intent: prepared.intent,
                    goal: prepared.goal,
                    policy: prepared.policy,
                    status: Status::AwaitingDelivery,
                    failure: None,
                    delivery,
                    lease,
                    pending: None,
                    permit_serial: None,
                    rebase_count: 0,
                    terminal_committed: false,
                    observed_revision: confirmed.revision(),
                    last_receipt_sequence: 0,
                    delivery_deadline: deadline,
                    run_deadline: 0,
                    last_progress: None,
                    active_ms: 0,
                    paused_at: None,
                },
            );
            if this.jobs[key.actor.as_str()].policy.presentation == Presentation::Visual {
                this.enqueue(&key, message)?;
            }
            Ok(key)
        })
    }
    fn job<'a>(
        &'a self,
        authority: &NativeAuthority,
        key: &JobKey,
    ) -> Result<&'a Job, ActionError> {
        let job = self
            .jobs
            .get(key.actor.as_str())
            .filter(|job| &job.key == key)
            .ok_or(ActionError::StaleReceipt)?;
        if &job.authority != authority {
            return Err(ActionError::WrongAuthority);
        }
        Ok(job)
    }
    fn confirmed_for(
        job: &Job,
        before: &LifeState,
        expected: ActionPhase,
    ) -> Result<(), ActionError> {
        before.validate().map_err(|_| ActionError::InvalidCommit)?;
        let action = before.current_action().ok_or(ActionError::InvalidCommit)?;
        if before.actor_id() != &job.key.actor
            || before.world_id() != job.key.action.world_id()
            || before.revision() < job.observed_revision
            || action.intent() != &job.intent
            || action.phase() != expected
        {
            return Err(ActionError::InvalidCommit);
        }
        Ok(())
    }
    fn request_commit(
        &mut self,
        key: &JobKey,
        before: &LifeState,
        event: LifeEvent,
        expected: ActionPhase,
    ) -> Result<Decision, ActionError> {
        let job = self
            .jobs
            .get(key.actor.as_str())
            .ok_or(ActionError::StaleReceipt)?;
        Self::confirmed_for(job, before, expected)?;
        if let Some(pending) = &job.pending {
            return if pending.event == event {
                Ok(Decision::Duplicate)
            } else {
                Err(ActionError::InvalidTransition)
            };
        }
        let request = CommitRequest {
            key: key.clone(),
            serial: self.serial()?,
            proposed_at: self.now,
            before: before.clone(),
            event,
        };
        self.jobs.get_mut(key.actor.as_str()).unwrap().pending = Some(request.clone());
        self.jobs.get_mut(key.actor.as_str()).unwrap().permit_serial = None;
        self.jobs.get_mut(key.actor.as_str()).unwrap().rebase_count = 0;
        Ok(Decision::Commit(request))
    }
    /// At most one same-action revision refresh after a controlled Store CAS
    /// conflict. It preserves the original proof/deadline and native policy;
    /// no new Plan, ordinal, duration or effect is allocated.
    pub fn refresh_commit_request(
        &mut self,
        authority: &NativeAuthority,
        request: &CommitRequest,
        current: &LifeState,
        raw: u64,
    ) -> Result<CommitRequest, ActionError> {
        self.transaction(|this| {
            this.clock(raw)?;
            let job = this.job(authority, &request.key)?;
            if job.pending.as_ref() != Some(request) || job.rebase_count >= 1 {
                return Err(ActionError::InvalidCommit);
            }
            if current == &request.before {
                return Ok(request.clone());
            }
            let phase = match &request.event {
                LifeEvent::Start { .. } => ActionPhase::Planned,
                LifeEvent::Complete { .. } => ActionPhase::Started,
                LifeEvent::Cancel { .. } => current
                    .current_action()
                    .ok_or(ActionError::InvalidCommit)?
                    .phase(),
                _ => return Err(ActionError::InvalidCommit),
            };
            Self::confirmed_for(job, current, phase)?;
            if current.revision() <= request.before.revision() {
                return Err(ActionError::InvalidCommit);
            }
            let next = CommitRequest {
                key: request.key.clone(),
                serial: this.serial()?,
                proposed_at: request.proposed_at,
                before: current.clone(),
                event: request.event.clone(),
            };
            let job = this.jobs.get_mut(request.key.actor.as_str()).unwrap();
            job.pending = Some(next.clone());
            job.permit_serial = None;
            job.rebase_count += 1;
            Ok(next)
        })
    }
    pub fn authorize_commit(
        &mut self,
        authority: &NativeAuthority,
        request: &CommitRequest,
        current: &LifeState,
        raw: u64,
    ) -> Result<CommitPermit, ActionError> {
        self.transaction(|this| {
            let now = this.clock(raw)?;
            let job = this.job(authority, &request.key)?;
            if job.pending.as_ref() != Some(request) || current != &request.before {
                return Err(ActionError::InvalidCommit);
            }
            let phase = match &request.event {
                LifeEvent::Start { .. } => {
                    if !matches!(job.status, Status::AwaitingDelivery | Status::Delivered)
                        || now >= job.delivery_deadline
                    {
                        return Err(ActionError::StaleReceipt);
                    }
                    ActionPhase::Planned
                }
                LifeEvent::Complete { .. } => {
                    let gap =
                        now.saturating_sub(job.last_progress.ok_or(ActionError::InvalidState)?);
                    if job.status != Status::Started
                        || now >= job.run_deadline
                        || gap > MAX_PROGRESS_GAP_MS
                    {
                        return Err(ActionError::StaleReceipt);
                    }
                    if job.active_ms.saturating_add(gap) < job.policy.minimum_active_ms {
                        return Err(ActionError::InvalidTransition);
                    }
                    ActionPhase::Started
                }
                LifeEvent::Cancel { .. } => {
                    if !matches!(job.status, Status::Cancelled | Status::Failed) {
                        return Err(ActionError::InvalidTransition);
                    }
                    current
                        .current_action()
                        .ok_or(ActionError::InvalidCommit)?
                        .phase()
                }
                _ => return Err(ActionError::InvalidCommit),
            };
            Self::confirmed_for(job, current, phase)?;
            let serial = this.serial()?;
            this.jobs
                .get_mut(request.key.actor.as_str())
                .unwrap()
                .permit_serial = Some(serial);
            Ok(CommitPermit {
                request: request.clone(),
                authority: authority.clone(),
                serial,
                admitted_at: now,
            })
        })
    }
    /// Native route must first authenticate the selected output and exact
    /// current session. Payload text can supply none of these authority values.
    pub fn receipt(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
        delivery: &str,
        sequence: u64,
        receipt: Receipt,
        before: &LifeState,
        raw: u64,
    ) -> Result<Decision, ActionError> {
        self.transaction(|this| {
            let now = this.clock(raw)?;
            let job = this.job(authority, key)?;
            if delivery != job.delivery || job.policy.presentation != Presentation::Visual {
                return Err(ActionError::StaleReceipt);
            }
            counter(sequence)?;
            if sequence <= job.last_receipt_sequence {
                return Ok(Decision::Duplicate);
            }
            if job.status.terminal() {
                return Ok(Decision::Duplicate);
            }
            if now >= job.delivery_deadline
                && matches!(job.status, Status::AwaitingDelivery | Status::Delivered)
            {
                return Err(ActionError::StaleReceipt);
            }
            this.jobs
                .get_mut(key.actor.as_str())
                .unwrap()
                .last_receipt_sequence = sequence;
            let job = this.job(authority, key)?;
            match receipt {
                Receipt::Delivered => {
                    if job.status != Status::AwaitingDelivery {
                        return Ok(Decision::Duplicate);
                    }
                    Self::confirmed_for(job, before, ActionPhase::Planned)?;
                    this.jobs.get_mut(key.actor.as_str()).unwrap().status = Status::Delivered;
                    Ok(Decision::Accepted)
                }
                Receipt::Started => {
                    if matches!(job.status, Status::Started | Status::Paused) {
                        return Ok(Decision::Duplicate);
                    }
                    if job.status != Status::Delivered {
                        return Err(ActionError::InvalidTransition);
                    }
                    this.request_commit(
                        key,
                        before,
                        LifeEvent::Start {
                            action_id: key.action.clone(),
                        },
                        ActionPhase::Planned,
                    )
                }
                Receipt::Progress => {
                    if job.status != Status::Started || job.pending.is_some() {
                        return Err(ActionError::InvalidTransition);
                    }
                    Self::confirmed_for(job, before, ActionPhase::Started)?;
                    let gap =
                        now.saturating_sub(job.last_progress.ok_or(ActionError::InvalidState)?);
                    if gap > MAX_PROGRESS_GAP_MS || now >= job.run_deadline {
                        this.finish(
                            key,
                            Status::Failed,
                            Some(Failure::Interrupted),
                            Some(CancelReason::Interrupted),
                        )?;
                    } else {
                        let job = this.jobs.get_mut(key.actor.as_str()).unwrap();
                        job.active_ms = job.active_ms.saturating_add(gap);
                        job.last_progress = Some(now);
                    }
                    Ok(Decision::Accepted)
                }
                Receipt::Completed(evidence) => {
                    if job.status != Status::Started || job.pending.is_some() {
                        return Err(ActionError::InvalidTransition);
                    }
                    if now >= job.run_deadline {
                        return Err(ActionError::StaleReceipt);
                    }
                    let gap =
                        now.saturating_sub(job.last_progress.ok_or(ActionError::InvalidState)?);
                    if gap > MAX_PROGRESS_GAP_MS {
                        this.finish(
                            key,
                            Status::Failed,
                            Some(Failure::Interrupted),
                            Some(CancelReason::Interrupted),
                        )?;
                        return Ok(Decision::Accepted);
                    }
                    let elapsed = job.active_ms.saturating_add(gap);
                    if elapsed < job.policy.minimum_active_ms {
                        return Err(ActionError::InvalidTransition);
                    }
                    let valid = match (job.policy.activity, evidence) {
                        (Activity::MoveTo, Evidence::Arrived(point)) => {
                            let target = job.goal.position();
                            (i64::from(point.x()) - i64::from(target.x())).abs()
                                <= ARRIVAL_TOLERANCE
                                && (i64::from(point.y()) - i64::from(target.y())).abs()
                                    <= ARRIVAL_TOLERANCE
                        }
                        (Activity::LookAt, Evidence::OrientationSettled) => true,
                        (
                            Activity::Rest | Activity::Recharge | Activity::Snack | Activity::Play | Activity::Socialize | Activity::Wave,
                            Evidence::VisualFinished,
                        ) => true,
                        _ => false,
                    };
                    if !valid {
                        return Err(ActionError::InvalidTransition);
                    }
                    this.request_commit(
                        key,
                        before,
                        LifeEvent::Complete {
                            action_id: key.action.clone(),
                        },
                        ActionPhase::Started,
                    )
                }
            }
        })
    }
    /// Explicit bounded nonvisual fallback. It reports no visual delivery or
    /// station arrival and can only start the opted-in zero-effect Rest policy.
    pub fn start_native_dwell(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
        before: &LifeState,
        raw: u64,
    ) -> Result<Decision, ActionError> {
        self.transaction(|this| {
            let now = this.clock(raw)?;
            let job = this.job(authority, key)?;
            if job.policy.presentation != Presentation::NativeRestDwell
                || now >= job.delivery_deadline
            {
                return Err(ActionError::UnsupportedCapability);
            }
            if job.status == Status::Started {
                return Ok(Decision::Duplicate);
            }
            if job.status != Status::AwaitingDelivery {
                return Err(ActionError::InvalidTransition);
            }
            this.request_commit(
                key,
                before,
                LifeEvent::Start {
                    action_id: key.action.clone(),
                },
                ActionPhase::Planned,
            )
        })
    }
    /// The host calls this only after a real, current native Store transaction.
    /// The receipt must match the entire frozen intent, source actor/world,
    /// expected prior revision, exact new revision/time and last committed event.
    pub fn confirm_commit(
        &mut self,
        authority: &NativeAuthority,
        permit: CommitPermit,
        confirmed: &LifeState,
        raw: u64,
    ) -> Result<(), ActionError> {
        self.transaction(|this| {
            let now = this.clock(raw)?;
            let request = &permit.request;
            let job = this.job(authority, &request.key)?;
            if &permit.authority != authority
                || permit.admitted_at < request.proposed_at
                || job.permit_serial != Some(permit.serial)
                || job.pending.as_ref() != Some(request)
                || confirmed.validate().is_err()
            {
                return Err(ActionError::InvalidCommit);
            }
            let action = confirmed
                .current_action()
                .ok_or(ActionError::InvalidCommit)?;
            if confirmed.actor_id() != &job.key.actor
                || confirmed.world_id() != job.key.action.world_id()
                || action.intent() != &job.intent
                || confirmed.revision()
                    != request
                        .before
                        .revision()
                        .checked_add(1)
                        .ok_or(ActionError::Exhausted)?
                || confirmed.simulation_time() != request.before.simulation_time()
            {
                return Err(ActionError::InvalidCommit);
            }
            let record = confirmed
                .recent_events()
                .last()
                .ok_or(ActionError::InvalidCommit)?;
            let previous = request
                .before
                .current_action()
                .ok_or(ActionError::InvalidCommit)?;
            let (phase, event) = match &request.event {
                LifeEvent::Start { action_id } => (
                    ActionPhase::Started,
                    CommittedEvent::Started {
                        action_id: action_id.clone(),
                    },
                ),
                LifeEvent::Complete { action_id } => (
                    ActionPhase::Completed,
                    CommittedEvent::Completed {
                        action_id: action_id.clone(),
                    },
                ),
                LifeEvent::Cancel { action_id, reason } => (
                    ActionPhase::Cancelled,
                    CommittedEvent::Cancelled {
                        action_id: action_id.clone(),
                        reason: *reason,
                    },
                ),
                _ => return Err(ActionError::InvalidCommit),
            };
            if action.phase() != phase
                || record.event() != &event
                || record.at() != confirmed.simulation_time()
            {
                return Err(ActionError::InvalidCommit);
            }
            let prior_sequence = request
                .before
                .recent_events()
                .last()
                .ok_or(ActionError::InvalidCommit)?
                .id()
                .sequence();
            if record.id().sequence()
                != prior_sequence
                    .checked_add(1)
                    .ok_or(ActionError::Exhausted)?
            {
                return Err(ActionError::InvalidCommit);
            }
            let timestamps_match = match &request.event {
                LifeEvent::Start { .. } => {
                    action.started_at() == Some(confirmed.simulation_time())
                        && action.finished_at().is_none()
                        && action.cancellation().is_none()
                }
                LifeEvent::Complete { .. } => {
                    action.started_at() == previous.started_at()
                        && action.finished_at() == Some(confirmed.simulation_time())
                        && action.cancellation().is_none()
                }
                LifeEvent::Cancel { reason, .. } => {
                    action.started_at() == previous.started_at()
                        && action.finished_at() == Some(confirmed.simulation_time())
                        && action.cancellation() == Some(*reason)
                }
                _ => false,
            };
            if !timestamps_match {
                return Err(ActionError::InvalidCommit);
            }
            let needs_match = if phase == ActionPhase::Completed {
                job.policy
                    .effect
                    .matches(request.before.needs(), confirmed.needs())
            } else {
                request.before.needs() == confirmed.needs()
            };
            if !needs_match {
                return Err(ActionError::InvalidCommit);
            }
            let job = this.jobs.get_mut(request.key.actor.as_str()).unwrap();
            job.pending = None;
            job.permit_serial = None;
            job.observed_revision = confirmed.revision();
            match phase {
                ActionPhase::Started => {
                    job.status = Status::Started;
                    job.run_deadline = request
                        .proposed_at
                        .checked_add(job.policy.maximum_run_ms)
                        .filter(|value| *value <= MAX_COUNTER)
                        .ok_or(ActionError::Exhausted)?;
                    job.last_progress = Some(request.proposed_at);
                    if now.saturating_sub(request.proposed_at) > MAX_PROGRESS_GAP_MS {
                        this.finish(
                            &request.key,
                            Status::Failed,
                            Some(Failure::Interrupted),
                            Some(CancelReason::Interrupted),
                        )?;
                    }
                }
                ActionPhase::Completed => {
                    this.finish(&request.key, Status::Completed, None, None)?;
                    this.jobs
                        .get_mut(request.key.actor.as_str())
                        .unwrap()
                        .terminal_committed = true;
                }
                ActionPhase::Cancelled => {
                    this.jobs
                        .get_mut(request.key.actor.as_str())
                        .unwrap()
                        .terminal_committed = true;
                } // immediate terminal tombstone was already published
                ActionPhase::Planned => return Err(ActionError::InvalidCommit),
            }
            Ok(())
        })
    }
    pub fn tick_native_dwell(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
        before: &LifeState,
        raw: u64,
    ) -> Result<Decision, ActionError> {
        self.transaction(|this| {
            let now = this.clock(raw)?;
            let job = this.job(authority, key)?;
            if job.policy.presentation != Presentation::NativeRestDwell
                || job.status != Status::Started
            {
                return Err(ActionError::InvalidTransition);
            }
            let gap = now.saturating_sub(job.last_progress.ok_or(ActionError::InvalidState)?);
            if gap > MAX_PROGRESS_GAP_MS || now >= job.run_deadline {
                this.finish(
                    key,
                    Status::Failed,
                    Some(Failure::Interrupted),
                    Some(CancelReason::Interrupted),
                )?;
                return Ok(Decision::Accepted);
            }
            let job = this.jobs.get_mut(key.actor.as_str()).unwrap();
            job.active_ms = job.active_ms.saturating_add(gap);
            job.last_progress = Some(now);
            if job.active_ms < job.policy.minimum_active_ms {
                return Ok(Decision::Accepted);
            }
            this.request_commit(
                key,
                before,
                LifeEvent::Complete {
                    action_id: key.action.clone(),
                },
                ActionPhase::Started,
            )
        })
    }
    pub fn pause(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
        raw: u64,
    ) -> Result<(), ActionError> {
        self.transaction(|this| {
            let now = this.clock(raw)?;
            let job = this.job(authority, key)?;
            if job.status == Status::Paused {
                return Ok(());
            }
            if job.status != Status::Started || job.pending.is_some() {
                return Err(ActionError::InvalidTransition);
            }
            let gap = now.saturating_sub(job.last_progress.ok_or(ActionError::InvalidState)?);
            if gap > MAX_PROGRESS_GAP_MS || now >= job.run_deadline {
                return this.finish(
                    key,
                    Status::Failed,
                    Some(Failure::Interrupted),
                    Some(CancelReason::Interrupted),
                );
            }
            let job = this.jobs.get_mut(key.actor.as_str()).unwrap();
            job.active_ms = job.active_ms.saturating_add(gap);
            job.last_progress = None;
            job.paused_at = Some(now);
            job.status = Status::Paused;
            let message = Message::Pause {
                delivery_id: job.delivery.clone(),
                action_id: key.action.clone(),
            };
            this.enqueue(key, message)
        })
    }
    pub fn resume(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
        raw: u64,
    ) -> Result<(), ActionError> {
        self.transaction(|this| {
            let now = this.clock(raw)?;
            let job = this.job(authority, key)?;
            if job.status != Status::Paused
                || now.saturating_sub(job.paused_at.ok_or(ActionError::InvalidState)?)
                    >= MAX_PAUSE_MS
                || now >= job.run_deadline
            {
                return Err(ActionError::InvalidTransition);
            }
            let job = this.jobs.get_mut(key.actor.as_str()).unwrap();
            job.status = Status::Started;
            job.last_progress = Some(now);
            job.paused_at = None;
            let message = Message::Resume {
                delivery_id: job.delivery.clone(),
                action_id: key.action.clone(),
            };
            this.enqueue(key, message)
        })
    }
    fn finish(
        &mut self,
        key: &JobKey,
        status: Status,
        failure: Option<Failure>,
        cancel_reason: Option<CancelReason>,
    ) -> Result<(), ActionError> {
        let job = self
            .jobs
            .get_mut(key.actor.as_str())
            .filter(|job| &job.key == key)
            .ok_or(ActionError::StaleReceipt)?;
        if job.status.terminal() {
            return Ok(());
        }
        job.status = status;
        job.failure = failure;
        job.pending = None;
        job.permit_serial = None;
        self.cleanup.push_back(Cleanup {
            key: key.clone(),
            lease: job.lease.take(),
            cancel_reason,
        });
        if self.cleanup.len() > MAX_JOBS {
            return Err(ActionError::Capacity);
        }
        let message = Message::Terminal {
            delivery_id: job.delivery.clone(),
            action_id: key.action.clone(),
            status,
            failure,
        };
        self.enqueue(key, message)
    }
    /// Stop invalidates presentation immediately. The returned Cleanup requests
    /// native Store Cancel; no unavailable write is represented as committed.
    pub fn cancel(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
        reason: CancelReason,
        raw: u64,
    ) -> Result<(), ActionError> {
        self.transaction(|this| {
            this.clock(raw)?;
            this.job(authority, key)?;
            if reason == CancelReason::Restart {
                return Err(ActionError::InvalidState);
            }
            this.finish(key, Status::Cancelled, None, Some(reason))
        })
    }
    pub fn fail(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
        failure: Failure,
        raw: u64,
    ) -> Result<(), ActionError> {
        self.transaction(|this| {
            this.clock(raw)?;
            this.job(authority, key)?;
            let reason = match failure {
                Failure::Timeout => CancelReason::Timeout,
                Failure::Interrupted => CancelReason::Interrupted,
                _ => CancelReason::Unavailable,
            };
            this.finish(key, Status::Failed, Some(failure), Some(reason))
        })
    }
    /// Rebase cancellation only from a fresh same-action native Store read.
    pub fn cancellation_commit(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
        before: &LifeState,
        reason: CancelReason,
    ) -> Result<Decision, ActionError> {
        self.transaction(|this| {
            let job = this.job(authority, key)?;
            if !matches!(job.status, Status::Cancelled | Status::Failed) {
                return Err(ActionError::InvalidTransition);
            }
            if reason == CancelReason::Restart {
                return Err(ActionError::InvalidState);
            }
            let phase = before
                .current_action()
                .ok_or(ActionError::InvalidCommit)?
                .phase();
            if !matches!(phase, ActionPhase::Planned | ActionPhase::Started) {
                return Err(ActionError::InvalidCommit);
            }
            this.request_commit(
                key,
                before,
                LifeEvent::Cancel {
                    action_id: key.action.clone(),
                    reason,
                },
                phase,
            )
        })
    }
    /// Reconcile the real native Stop path when it already persisted Cancel
    /// outside this executor. This accepts only that same exact terminal action;
    /// it cannot synthesize Complete, reward, a new action or a replacement job.
    pub fn confirm_native_cancelled(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
        confirmed: &LifeState,
        raw: u64,
    ) -> Result<(), ActionError> {
        self.transaction(|this| {
            this.clock(raw)?;
            let job = this.job(authority, key)?;
            if !matches!(job.status, Status::Cancelled | Status::Failed)
                || confirmed.validate().is_err()
            {
                return Err(ActionError::InvalidCommit);
            }
            let action = confirmed
                .current_action()
                .ok_or(ActionError::InvalidCommit)?;
            if confirmed.actor_id() != &key.actor
                || confirmed.world_id() != key.action.world_id()
                || action.intent() != &job.intent
                || action.phase() != ActionPhase::Cancelled
                || confirmed.revision() <= job.observed_revision
            {
                return Err(ActionError::InvalidCommit);
            }
            let job = this.jobs.get_mut(key.actor.as_str()).unwrap();
            job.pending = None;
            job.permit_serial = None;
            job.terminal_committed = true;
            job.observed_revision = confirmed.revision();
            Ok(())
        })
    }
    pub fn advance(&mut self, raw: u64) -> Result<(), ActionError> {
        self.transaction(|this| {
            let now = this.clock(raw)?;
            let expired = this
                .jobs
                .values()
                .filter_map(|job| {
                    if job.status.terminal() {
                        return None;
                    }
                    let reason = match job.status {
                        Status::AwaitingDelivery | Status::Delivered => {
                            (now >= job.delivery_deadline).then_some(Failure::Timeout)
                        }
                        Status::Paused => (now.saturating_sub(job.paused_at.unwrap_or(now))
                            >= MAX_PAUSE_MS
                            || now >= job.run_deadline)
                            .then_some(Failure::Timeout),
                        Status::Started if now >= job.run_deadline => Some(Failure::Timeout),
                        Status::Started
                            if now.saturating_sub(job.last_progress.unwrap_or(now))
                                > MAX_PROGRESS_GAP_MS =>
                        {
                            Some(Failure::Interrupted)
                        }
                        _ => None,
                    };
                    reason.map(|reason| (job.key.clone(), reason))
                })
                .collect::<Vec<_>>();
            for (key, failure) in expired {
                this.finish(
                    &key,
                    Status::Failed,
                    Some(failure),
                    Some(if failure == Failure::Timeout {
                        CancelReason::Timeout
                    } else {
                        CancelReason::Interrupted
                    }),
                )?;
            }
            Ok(())
        })
    }
    pub fn status(&self, key: &JobKey) -> Option<Status> {
        self.jobs
            .get(key.actor.as_str())
            .filter(|job| &job.key == key)
            .map(|job| job.status)
    }
    pub fn snapshot(&self, key: &JobKey) -> Option<JobSnapshot> {
        self.jobs
            .get(key.actor.as_str())
            .filter(|job| &job.key == key)
            .map(|job| JobSnapshot {
                version: 1,
                action_id: job.key.action.clone(),
                activity: job.policy.activity,
                status: job.status,
                failure: job.failure,
                presentation: job.policy.presentation,
                active_ms: job.active_ms,
                terminal_committed: job.terminal_committed,
            })
    }
    pub fn take_dispatch(&mut self) -> Option<Dispatch> {
        self.outbox.pop_front()
    }
    pub fn take_cleanup(&mut self) -> Option<Cleanup> {
        self.cleanup.pop_front()
    }
    pub fn take_invalidated_output(&mut self) -> Option<OutputStamp> {
        self.invalidated_outputs.pop_first()
    }
    /// Native host only, after exact Core/session, World and internal timer
    /// output revocation. This retires failed runtime authority, NEVER Life
    /// history/ordinals. No renderer/API boolean reaches this method. Old keys
    /// and permits fail missing-job lookup; queued terminal tombstones survive.
    pub fn retire_uncommitted_native_rest(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
    ) -> Result<(), ActionError> {
        let job = self.job(authority, key)?;
        if job.policy.presentation != Presentation::NativeRestDwell
            || !matches!(job.status, Status::Cancelled | Status::Failed)
            || job.terminal_committed
            || self.cleanup.iter().any(|item| item.key == *key)
        {
            return Err(ActionError::InvalidState);
        }
        self.jobs.remove(key.actor.as_str());
        Ok(())
    }
    /// Native host only, after exact Core/World/registered-output revocation.
    /// No wire receipt/boolean may supply that proof; terminal outbox is retained.
    pub(crate) fn retire_uncommitted_visual(
        &mut self,
        authority: &NativeAuthority,
        key: &JobKey,
    ) -> Result<(), ActionError> {
        let job = self.job(authority, key)?;
        if job.policy.presentation != Presentation::Visual
            || !matches!(job.status, Status::Cancelled | Status::Failed)
            || job.terminal_committed
            || self.cleanup.iter().any(|item| item.key == *key)
        {
            return Err(ActionError::InvalidState);
        }
        self.jobs.remove(key.actor.as_str());
        Ok(())
    }
    /// Membership removal may forget only terminal work after native cleanup;
    /// queued terminal tombstones remain ordered until dispatched.
    pub fn forget_terminal(&mut self, actor: &ActorId) -> bool {
        if self.jobs.get(actor.as_str()).is_some_and(|job| {
            job.status.terminal() && job.terminal_committed && job.pending.is_none()
        }) && !self.cleanup.iter().any(|item| item.key.actor == *actor)
        {
            self.jobs.remove(actor.as_str());
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
#[path = "action_executor/tests.rs"]
mod tests;
