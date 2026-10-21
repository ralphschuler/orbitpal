// SPDX-License-Identifier: MIT
/** Pure World motion adapter. Inputs are authorized LOCAL
 * CSS targets: no canonical/DPI conversion, DOM, native APIs, timers or random
 * goals. A mathematical target is never a successful-render/arrival receipt. */
import type { CssPoint as WirePoint, CssSize as WireSize, CssRect as WireRect,
  ActionId as WireActionId, GroundBasis as WireBasis, MotionBinding as WireBinding,
  MotionFence as WireFence, EnvelopeExtents, LocalReference, PetClip } from './world-output';
export type CssPoint = Readonly<WirePoint>;
export type CssSize = Readonly<WireSize>;
export type CssRect = Readonly<WireRect>;
export type GroundPoint = Readonly<{ x: number; z: number }>;
export type ActionId = Readonly<WireActionId>;
export type GroundBasis = Readonly<WireBasis>;
export type MotionBinding = Readonly<WireBinding>;
export type MotionFence = Readonly<WireFence>;
export const STATION_ACTIVITIES = ['rest', 'recharge', 'snack', 'play', 'socialize'] as const;
export type StationActivity = typeof STATION_ACTIVITIES[number];
type Header = Readonly<{ version: 1; sequence: number; fence: MotionFence }>;
export type MotionOffer = Header & Readonly<{
  type: 'offer'; activity: 'move_to' | StationActivity | 'pet'; from: CssPoint; target: CssPoint;
  maximumSpeed: number; acceleration: number; validForMs: number;
  minimumActiveMs: number;
  petClip?: PetClip;
}>;
export type WorldMotionCommand = MotionOffer
  | (Header & Readonly<{ type: 'pause' | 'resume' }>)
  | (Header & Readonly<{ type: 'terminal'; status: 'completed' | 'cancelled' | 'failed' }>);
export type MotionPhase = 'unavailable' | 'idle' | 'offered' | 'running' | 'paused' | 'at_target';
export type MotionReason = 'unregistered' | 'invalid_binding' | 'binding_changed' | 'invalid_clock'
  | 'frame_gap' | 'render_failed' | 'invalid_render' | 'unsupported_walk' | 'unsupported_activity' | 'reduced_motion'
  | 'conflicting_offer' | 'superseded' | 'deadline' | 'disconnected' | 'counter_exhausted';
export type ReceiveResult = 'accepted' | 'duplicate' | 'stale' | 'rejected';
export type Direction8 = 'right' | 'down_right' | 'down' | 'down_left' | 'left' | 'up_left' | 'up' | 'up_right';

export const WORLD_MAX_FRAME_SECONDS = 0.05;
export const WORLD_MAX_FRAME_GAP_MS = 250;
export const WORLD_MAX_RENDER_AGE_MS = 250;
export const WORLD_MAX_SPEED = 500;
const MAX_DIMENSION = 1_000_000;
const MAX_COUNTER = Number.MAX_SAFE_INTEGER;
const MAX_TRAVEL = 1_000_000_000;
const POSITION_EPSILON = 1e-7;
const RENDER_TOLERANCE = 0.25;
const directions: readonly Direction8[] = ['right', 'down_right', 'down', 'down_left', 'left', 'up_left', 'up', 'up_right'];
function directionOf(travel: CssPoint): Direction8 | null {
  return Math.hypot(travel.x, travel.y) > POSITION_EPSILON
    ? directions[(Math.round(Math.atan2(travel.y, travel.x) / (Math.PI / 4)) + 8) % 8] : null;
}
const fenceKeys = ['actorId', 'renderSessionId', 'bindingSequence', 'worldInstanceId', 'stageId',
  'configRevision', 'topologyRevision', 'layoutRevision', 'assetRevision', 'measurementRevision', 'referenceRevision'] as const;
const bindingKeys = [...fenceKeys, 'sourceWorldId', 'viewport', 'safeCenters', 'maximumFootprint',
  'position', 'groundBasis', 'modelForwardYaw', 'scale', 'reducedMotion', 'walkSupported',
  'referenceLocal', 'envelopeExtents', 'profileScale', 'effectiveScale'] as const;
const clamp = (value: number, minimum: number, maximum: number) => Math.min(maximum, Math.max(minimum, value));
const finite = (value: unknown, minimum: number, maximum: number): value is number => typeof value === 'number' && Number.isFinite(value) && value >= minimum && value <= maximum;
const counter = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) > 0 && Number(value) <= MAX_COUNTER;
const id = (value: unknown): value is string => typeof value === 'string' && /^[A-Za-z0-9_-]{1,96}$/.test(value);
const copyPoint = (value: CssPoint): CssPoint => Object.freeze({ x: value.x, y: value.y });
function fields(value: unknown, keys: readonly string[]): Record<string, unknown> | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) return null;
  const descriptors = Object.getOwnPropertyDescriptors(value);
  if (Reflect.ownKeys(descriptors).length !== keys.length || keys.some(key => !Object.hasOwn(descriptors, key) || !Object.hasOwn(descriptors[key], 'value'))) return null;
  return Object.fromEntries(keys.map(key => [key, descriptors[key].value]));
}
function point(value: unknown): CssPoint | null {
  const data = fields(value, ['x', 'y']);
  return data && finite(data.x, 0, MAX_DIMENSION) && finite(data.y, 0, MAX_DIMENSION) ? copyPoint({ x: data.x, y: data.y }) : null;
}
function size(value: unknown): CssSize | null {
  const data = fields(value, ['width', 'height']);
  return data && finite(data.width, Number.MIN_VALUE, MAX_DIMENSION) && finite(data.height, Number.MIN_VALUE, MAX_DIMENSION)
    ? Object.freeze({ width: data.width, height: data.height }) : null;
}
function rect(value: unknown): CssRect | null {
  const data = fields(value, ['left', 'top', 'right', 'bottom']);
  if (!data || ![data.left, data.top, data.right, data.bottom].every(value => finite(value, 0, MAX_DIMENSION))) return null;
  const result = data as unknown as CssRect;
  return result.left < result.right && result.top < result.bottom ? Object.freeze({ ...result }) : null;
}
function basis(value: unknown): GroundBasis | null {
  const data = fields(value, ['right', 'down']);
  if (!data) return null;
  const right = fields(data.right, ['x', 'z']), down = fields(data.down, ['x', 'z']);
  if (!right || !down || ![right.x, right.z, down.x, down.z].every(value => finite(value, -10_000, 10_000))) return null;
  const r = right as unknown as GroundPoint, d = down as unknown as GroundPoint;
  if (Math.abs(r.x * d.z - r.z * d.x) < 1e-12) return null;
  return Object.freeze({ right: Object.freeze({ ...r }), down: Object.freeze({ ...d }) });
}
function reference(value: unknown): Readonly<LocalReference> | null {
  const data = fields(value, ['x', 'y', 'z']);
  return data && [data.x, data.y, data.z].every(n => finite(n, -MAX_DIMENSION, MAX_DIMENSION))
    ? Object.freeze({ ...data }) as Readonly<LocalReference> : null;
}
function envelope(value: unknown): Readonly<EnvelopeExtents> | null {
  const data = fields(value, ['left', 'top', 'right', 'bottom']);
  return data && [data.left, data.top, data.right, data.bottom].every(n => finite(n, 0, MAX_DIMENSION))
    ? Object.freeze({ ...data }) as Readonly<EnvelopeExtents> : null;
}
export function containsPosedEnvelope(anchor: CssPoint, posed: CssRect, extents: Readonly<EnvelopeExtents>, tolerance = 0): boolean {
  return posed.left >= anchor.x - extents.left - tolerance && posed.right <= anchor.x + extents.right + tolerance
    && posed.top >= anchor.y - extents.top - tolerance && posed.bottom <= anchor.y + extents.bottom + tolerance;
}
function inside(position: CssPoint, bounds: CssRect): boolean {
  return position.x >= bounds.left && position.x <= bounds.right && position.y >= bounds.top && position.y <= bounds.bottom;
}
function samePoint(first: CssPoint, second: CssPoint, tolerance = POSITION_EPSILON): boolean {
  return Math.hypot(first.x - second.x, first.y - second.y) <= tolerance;
}
function sameAction(first: MotionFence, second: MotionFence): boolean {
  return first.deliveryId === second.deliveryId && first.actionId.actorId === second.actionId.actorId
    && first.actionId.worldId === second.actionId.worldId && first.actionId.ordinal === second.actionId.ordinal;
}
function sameFence(first: MotionFence | MotionBinding, second: MotionFence | MotionBinding): boolean {
  return fenceKeys.every(key => first[key] === second[key]);
}
function actionId(value: unknown): ActionId | null {
  const data = fields(value, ['actorId', 'worldId', 'ordinal']);
  return data && id(data.actorId) && id(data.worldId) && counter(data.ordinal)
    ? Object.freeze({ actorId: data.actorId, worldId: data.worldId, ordinal: data.ordinal }) : null;
}
function fence(value: unknown): MotionFence | null {
  const data = fields(value, [...fenceKeys, 'actionId', 'deliveryId']);
  if (!data || !['actorId', 'renderSessionId', 'worldInstanceId', 'stageId', 'deliveryId'].every(key => id(data[key]))
    || !['bindingSequence', 'configRevision', 'topologyRevision', 'layoutRevision', 'assetRevision', 'measurementRevision', 'referenceRevision'].every(key => counter(data[key]))) return null;
  const action = actionId(data.actionId);
  if (!action || action.actorId !== data.actorId) return null;
  return Object.freeze({ ...data, actionId: action }) as MotionFence;
}
export function parseMotionBinding(value: unknown): MotionBinding | null {
  const hasStation = value !== null && typeof value === 'object' && Object.hasOwn(value, 'stationSupported');
  const data = fields(value, [...bindingKeys, ...(hasStation ? ['stationSupported'] : [])]);
  if (!data || !['actorId', 'sourceWorldId', 'renderSessionId', 'worldInstanceId', 'stageId'].every(key => id(data[key]))
    || !['bindingSequence', 'configRevision', 'topologyRevision', 'layoutRevision', 'assetRevision', 'measurementRevision', 'referenceRevision'].every(key => counter(data[key]))
    || typeof data.reducedMotion !== 'boolean' || typeof data.walkSupported !== 'boolean'
    || hasStation && typeof data.stationSupported !== 'boolean'
    || !finite(data.modelForwardYaw, -Math.PI, Math.PI) || !finite(data.scale, 0.01, 100)
    || !finite(data.profileScale, .3, 2.5) || !finite(data.effectiveScale, .3, 2.5)) return null;
  const viewport = size(data.viewport), safeCenters = rect(data.safeCenters), maximumFootprint = size(data.maximumFootprint), position = point(data.position), groundBasis = basis(data.groundBasis);
  const referenceLocal = reference(data.referenceLocal), envelopeExtents = envelope(data.envelopeExtents);
  if (!viewport || !safeCenters || !maximumFootprint || !position || !groundBasis || !referenceLocal || !envelopeExtents
    || Math.abs(maximumFootprint.width - 2 * Math.max(envelopeExtents.left, envelopeExtents.right)) > 1e-7
    || Math.abs(maximumFootprint.height - 2 * Math.max(envelopeExtents.top, envelopeExtents.bottom)) > 1e-7
    || safeCenters.right > viewport.width || safeCenters.bottom > viewport.height || !inside(position, safeCenters)
    // maximumFootprint remains symmetric metadata. Feasibility follows the
    // actual all-pose asymmetric envelope around the immutable rig reference.
    || envelopeExtents.left + envelopeExtents.right > viewport.width
    || envelopeExtents.top + envelopeExtents.bottom > viewport.height
    || safeCenters.left < envelopeExtents.left || safeCenters.top < envelopeExtents.top
    || safeCenters.right > viewport.width - envelopeExtents.right
    || safeCenters.bottom > viewport.height - envelopeExtents.bottom) return null;
  return Object.freeze({ ...data, stationSupported: hasStation ? data.stationSupported : false,
    viewport, safeCenters, maximumFootprint, position, groundBasis, referenceLocal, envelopeExtents }) as MotionBinding;
}
export function parseWorldMotionCommand(value: unknown): WorldMotionCommand | null {
  if (typeof value !== 'object' || value === null) return null;
  const typeDescriptor = Object.getOwnPropertyDescriptor(value, 'type');
  const type = typeDescriptor && Object.hasOwn(typeDescriptor, 'value') ? typeDescriptor.value : undefined;
  const common = ['version', 'sequence', 'fence', 'type'];
  const hasPet = Object.hasOwn(value, 'petClip');
  const keys = type === 'offer' ? [...common, 'activity', 'from', 'target', 'maximumSpeed', 'acceleration', 'validForMs', 'minimumActiveMs', ...(hasPet ? ['petClip'] : [])]
    : type === 'terminal' ? [...common, 'status'] : type === 'pause' || type === 'resume' ? common : null;
  const data = keys && fields(value, keys);
  if (!data || data.version !== 1 || !counter(data.sequence)) return null;
  const identity = fence(data.fence);
  if (!identity) return null;
  if (type === 'offer') {
    const from = point(data.from), target = point(data.target);
    const activity = data.activity === 'move_to' || data.activity === 'pet' || STATION_ACTIVITIES.includes(data.activity as StationActivity);
    const rawPet = hasPet ? fields(data.petClip, ['action', 'durationMs']) : null;
    const petClip = rawPet && typeof rawPet.action === 'string' && /^pet$/i.test(rawPet.action)
      && Number.isSafeInteger(rawPet.durationMs) && finite(rawPet.durationMs, 1, 15000)
      ? Object.freeze({ action: rawPet.action, durationMs: rawPet.durationMs }) as PetClip : null;
    if (data.activity === 'pet' ? !petClip || !from || !target || !samePoint(from, target)
      || Number(data.minimumActiveMs) !== Math.max(200, petClip.durationMs) : hasPet) return null;
    const minimum = Number.isSafeInteger(data.minimumActiveMs) && finite(data.minimumActiveMs, 0, 600_000)
      && (data.activity === 'move_to' ? data.minimumActiveMs === 0 : data.minimumActiveMs > 0)
      && Number(data.minimumActiveMs) < Number(data.validForMs);
    return activity && minimum && from && target && finite(data.maximumSpeed, 0.01, WORLD_MAX_SPEED)
      && finite(data.acceleration, 0.01, 5000) && counter(data.validForMs)
      && data.validForMs <= (data.activity === 'move_to' ? 75_000
        : Math.max(data.activity === 'recharge' ? 40_000 : 30_000, Number(data.minimumActiveMs) + 15_000))
      ? Object.freeze({ ...data, ...(petClip ? {petClip} : {}), from, target, fence: identity }) as MotionOffer : null;
  }
  if (type === 'terminal' && (typeof data.status !== 'string' || !['completed', 'cancelled', 'failed'].includes(data.status))) return null;
  return Object.freeze({ ...data, fence: identity }) as WorldMotionCommand;
}
/** Exact opaque frame object identity, never serialized to native or accepted
 * from a renderer/model message. Only its owning follower can commit it. */
export interface RenderToken { readonly frame: number }
export type MotionProposal = Readonly<{
  token: RenderToken; fence: MotionFence; position: CssPoint; travel: CssPoint;
  groundTravel: GroundPoint; groundDistance: number; cssDistance: number;
  desiredYaw: number; yaw: number; direction: Direction8 | null;
  walking: boolean; mathematicallyAtTarget: boolean;
}>;
export type RenderEvidence = Readonly<{
  fence: MotionFence; frameSequence: number; now: number; position: CssPoint;
  viewport: CssSize; posedBounds: CssRect;
}>;
export type FrameObservation = Readonly<{
  fence: MotionFence; frameSequence: number; position: CssPoint;
  travel: CssPoint; groundDistance: number; totalGroundDistance: number;
  direction: Direction8 | null; yaw: number; walking: boolean;
  targetIdleFrames: number;
}>;
export type MotionSnapshot = Readonly<{ phase: MotionPhase; reason: MotionReason | null;
  position: CssPoint | null; yaw: number; groundDistance: number; lastSequence: number }>;
type Pending = { proposal: MotionProposal; speed: number; generatedAt: number; work: number };
type Work = { offer: MotionOffer; deadline: number; generation: number; delivered: boolean };

export class WorldMotionFollower {
  private binding: MotionBinding | null = null;
  private bindingHighwater = 0;
  private sourceWorld: string | null = null;
  private renderSession: string | null = null;
  private lastSequence = 0;
  private terminalOrdinal = 0;
  private phase: MotionPhase = 'unavailable';
  private reason: MotionReason | null = 'unregistered';
  private work: Work | null = null;
  private generation = 0;
  private frame = 0;
  private lastFrameSequence = 0;
  private now = 0;
  private sampledAt: number | null = null;
  private position: CssPoint | null = null;
  private speed = 0;
  private yaw = 0;
  private groundDistance = 0;
  private targetIdleFrames = 0;
  private pending: Pending | null = null;
  private disposed = false;

  private clock(raw: number): number | null {
    if (!finite(raw, 0, MAX_COUNTER)) { this.invalidate('invalid_clock'); return null; }
    this.now = Math.max(this.now, raw);
    return this.now;
  }
  private change(): boolean {
    if (this.generation >= MAX_COUNTER) { this.invalidate('counter_exhausted'); return false; }
    this.generation++;
    this.pending = null;
    return true;
  }
  /** Caller may bind only the latest actual native registration response. A
   * new native connection must use a fresh follower; dispose the old one. */
  bind(value: unknown, raw: number): boolean {
    if (this.disposed || this.clock(raw) === null) return false;
    const binding = parseMotionBinding(value);
    if (!binding) { this.invalidate('invalid_binding'); return false; }
    if (binding.bindingSequence <= this.bindingHighwater) return false;
    this.invalidate('binding_changed');
    this.bindingHighwater = binding.bindingSequence;
    if (this.renderSession !== binding.renderSessionId) {
      this.lastSequence = 0;
      this.lastFrameSequence = 0;
    }
    if (this.sourceWorld !== binding.sourceWorldId) this.terminalOrdinal = 0;
    this.renderSession = binding.renderSessionId;
    this.sourceWorld = binding.sourceWorldId;
    this.binding = binding;
    this.position = copyPoint(binding.position);
    this.phase = 'idle'; this.reason = null; this.speed = 0; this.yaw = 0;
    this.groundDistance = 0; this.targetIdleFrames = 0; this.sampledAt = this.now;
    return true;
  }
  receive(value: unknown, raw: number): ReceiveResult {
    const now = this.clock(raw), command = parseWorldMotionCommand(value), binding = this.binding;
    if (this.disposed || now === null || !command || !binding || !sameFence(command.fence, binding)
      || command.fence.actionId.worldId !== binding.sourceWorldId) return 'stale';
    if (command.sequence <= this.lastSequence) return 'stale';
    if (command.type === 'terminal') {
      this.lastSequence = command.sequence;
      this.terminalOrdinal = Math.max(this.terminalOrdinal, command.fence.actionId.ordinal);
      if (this.work && command.fence.actionId.ordinal >= this.work.offer.fence.actionId.ordinal) this.stopWork(null);
      return 'accepted';
    }
    if (command.type === 'offer') {
      if (command.fence.actionId.ordinal <= this.terminalOrdinal) return 'stale';
      if (this.work && sameAction(this.work.offer.fence, command.fence)) {
        const current = this.work.offer;
        if (!samePoint(current.from, command.from) || !samePoint(current.target, command.target)
          || current.activity !== command.activity || current.minimumActiveMs !== command.minimumActiveMs
          || JSON.stringify(current.petClip) !== JSON.stringify(command.petClip)
          || current.maximumSpeed !== command.maximumSpeed || current.acceleration !== command.acceleration || current.validForMs !== command.validForMs) {
          this.lastSequence = command.sequence; this.stopWork('conflicting_offer'); return 'rejected';
        }
        this.lastSequence = command.sequence;
        return 'duplicate';
      }
      if (this.work && command.fence.actionId.ordinal <= this.work.offer.fence.actionId.ordinal) return 'stale';
      this.lastSequence = command.sequence;
      if (!binding.walkSupported || binding.reducedMotion) { this.stopWork(binding.reducedMotion ? 'reduced_motion' : 'unsupported_walk'); this.terminalOrdinal = Math.max(this.terminalOrdinal, command.fence.actionId.ordinal); return 'rejected'; }
      if (command.activity !== 'move_to' && !binding.stationSupported) { this.stopWork('unsupported_activity'); this.terminalOrdinal = Math.max(this.terminalOrdinal, command.fence.actionId.ordinal); return 'rejected'; }
      if (!this.position || !samePoint(this.position, command.from, RENDER_TOLERANCE) || !inside(command.from, binding.safeCenters) || !inside(command.target, binding.safeCenters)) { this.stopWork('invalid_render'); this.terminalOrdinal = Math.max(this.terminalOrdinal, command.fence.actionId.ordinal); return 'rejected'; }
      if (this.work) this.stopWork('superseded');
      if (!this.change()) return 'rejected';
      const deadline = now + command.validForMs;
      if (!Number.isSafeInteger(Math.ceil(deadline)) || deadline > MAX_COUNTER) { this.invalidate('counter_exhausted'); return 'rejected'; }
      this.work = { offer: command, deadline, generation: this.generation, delivered: false };
      this.phase = 'offered'; this.reason = null; this.speed = 0; this.groundDistance = 0; this.targetIdleFrames = 0; this.sampledAt = now;
      return 'accepted';
    }
    if (!this.work || !sameAction(this.work.offer.fence, command.fence)) return 'stale';
    this.lastSequence = command.sequence;
    if (command.type === 'pause') {
      if (this.phase === 'offered') { this.stopWork('superseded'); return 'accepted'; }
      if (!this.change()) return 'rejected';
      this.phase = 'paused'; this.speed = 0; this.targetIdleFrames = 0; this.sampledAt = now;
      return 'accepted';
    }
    if (this.phase !== 'paused' || now >= this.work.deadline) return 'stale';
    if (!this.change()) return 'rejected';
    this.work.generation = this.generation;
    this.phase = 'running'; this.sampledAt = now; this.reason = null;
    return 'accepted';
  }
  /** Native Delivered acceptance must refer to the exact original Offer. A late
   * promise cannot start a paused/cancelled/replaced presentation. */
  beginAfterDelivered(identity: MotionFence, offerSequence: number, raw: number): boolean {
    const work = this.work;
    if (this.disposed || !work || this.phase !== 'offered' || !sameFence(work.offer.fence, identity)
      || !sameAction(work.offer.fence, identity) || work.offer.sequence !== offerSequence) return false;
    const now = this.clock(raw);
    if (now === null || now >= work.deadline) return false;
    work.delivered = true; this.phase = 'running'; this.sampledAt = now;
    return true;
  }
  sample(raw: number, deltaSeconds: number): MotionProposal | null {
    const now = this.clock(raw), binding = this.binding, work = this.work;
    if (this.disposed || now === null || !binding || !work || !this.position || !work.delivered) return null;
    if (now >= work.deadline) { this.stopWork('deadline'); return null; }
    const elapsedMs = this.sampledAt === null ? 0 : now - this.sampledAt;
    if (elapsedMs > WORLD_MAX_FRAME_GAP_MS) { this.stopWork('frame_gap'); return null; }
    if (this.pending) return this.pending.proposal;
    this.sampledAt = now;
    const delta = finite(deltaSeconds, 0, 1) ? Math.min(deltaSeconds, elapsedMs / 1000, WORLD_MAX_FRAME_SECONDS) : 0;
    const paused = this.phase === 'paused';
    const dx = work.offer.target.x - this.position.x, dy = work.offer.target.y - this.position.y;
    const remaining = Math.hypot(dx, dy);
    const settled = remaining <= RENDER_TOLERANCE;
    const desired = paused || settled ? 0 : Math.min(work.offer.maximumSpeed, Math.sqrt(2 * work.offer.acceleration * remaining));
    const speed = paused ? 0 : Math.max(0, this.speed + clamp(desired - this.speed, -work.offer.acceleration * delta, work.offer.acceleration * delta));
    const distance = paused || settled ? 0 : Math.min(remaining, speed * delta);
    const travel = remaining > 0 ? { x: dx / remaining * distance, y: dy / remaining * distance } : { x: 0, y: 0 };
    const position = { x: this.position.x + travel.x, y: this.position.y + travel.y };
    const ground = { x: travel.x * binding.groundBasis.right.x + travel.y * binding.groundBasis.down.x,
      z: travel.x * binding.groundBasis.right.z + travel.y * binding.groundBasis.down.z };
    const groundDistance = Math.hypot(ground.x, ground.z), moving = distance > POSITION_EPSILON;
    const desiredYaw = moving ? Math.atan2(ground.x, ground.z) - binding.modelForwardYaw : this.yaw;
    const angle = Math.atan2(Math.sin(desiredYaw - this.yaw), Math.cos(desiredYaw - this.yaw));
    const yaw = moving ? this.yaw + angle * (1 - Math.exp(-delta * 9)) : this.yaw;
    const direction = directionOf(travel);
    if (this.frame >= MAX_COUNTER || ![position.x, position.y, groundDistance, yaw].every(Number.isFinite)) { this.invalidate('counter_exhausted'); return null; }
    const token = Object.freeze({ frame: ++this.frame });
    const proposal: MotionProposal = Object.freeze({ token, fence: work.offer.fence, position: copyPoint(position), travel: copyPoint(travel), groundTravel: Object.freeze(ground), groundDistance, cssDistance: distance, desiredYaw, yaw, direction, walking: moving, mathematicallyAtTarget: samePoint(position, work.offer.target, RENDER_TOLERANCE) });
    this.pending = { proposal, speed: remaining - distance <= RENDER_TOLERANCE ? 0 : speed, generatedAt: now, work: this.generation };
    return proposal;
  }
  /** Invoke ONLY after the actual renderer completed this exact proposed frame
   * successfully and measured its actual current viewport/posed bounds. This is
   * local observation; native independently validates arrival/lease/effects. */
  afterSuccessfulRender(token: RenderToken, evidence: RenderEvidence): FrameObservation | null {
    const pending = this.pending, binding = this.binding, work = this.work;
    if (this.disposed || !pending || token !== pending.proposal.token || !binding || !work
      || pending.work !== this.generation || !sameFence(evidence.fence, work.offer.fence) || !sameAction(evidence.fence, work.offer.fence)) return null;
    const now = this.clock(evidence.now);
    if (now === null) return null;
    const actual = point(evidence.position), viewport = size(evidence.viewport), bounds = rect(evidence.posedBounds);
    if (!counter(evidence.frameSequence) || evidence.frameSequence <= this.lastFrameSequence || now - pending.generatedAt > WORLD_MAX_RENDER_AGE_MS || now >= work.deadline
      || !actual || !viewport || !bounds || !samePoint(actual, pending.proposal.position, RENDER_TOLERANCE)
      || viewport.width !== binding.viewport.width || viewport.height !== binding.viewport.height
      || bounds.right > viewport.width || bounds.bottom > viewport.height
      || bounds.right - bounds.left > binding.maximumFootprint.width + RENDER_TOLERANCE
      || bounds.bottom - bounds.top > binding.maximumFootprint.height + RENDER_TOLERANCE
      || !containsPosedEnvelope(actual, bounds, binding.envelopeExtents, RENDER_TOLERANCE)
      || !inside(actual, binding.safeCenters)) { this.stopWork('invalid_render'); return null; }
    const travel = { x: actual.x - this.position!.x, y: actual.y - this.position!.y };
    const ground = { x: travel.x * binding.groundBasis.right.x + travel.y * binding.groundBasis.down.x,
      z: travel.x * binding.groundBasis.right.z + travel.y * binding.groundBasis.down.z };
    const actualDistance = Math.hypot(ground.x, ground.z);
    const actualCssDistance = Math.hypot(travel.x, travel.y);
    const actualWalking = actualCssDistance > POSITION_EPSILON;
    if (!pending.proposal.walking && Math.hypot(travel.x, travel.y) > POSITION_EPSILON) { this.stopWork('invalid_render'); return null; }
    const total = this.groundDistance + actualDistance;
    if (!Number.isFinite(total) || total > MAX_TRAVEL) { this.invalidate('counter_exhausted'); return null; }
    const speedRatio = pending.proposal.cssDistance > POSITION_EPSILON ? Math.min(1, actualCssDistance / pending.proposal.cssDistance) : 1;
    this.position = copyPoint(actual); this.speed = pending.speed * speedRatio; this.yaw = pending.proposal.yaw;
    this.groundDistance = total; this.lastFrameSequence = evidence.frameSequence;
    this.sampledAt = now;
    this.targetIdleFrames = samePoint(actual, work.offer.target, RENDER_TOLERANCE) && !actualWalking && this.phase !== 'paused' ? Math.min(2, this.targetIdleFrames + 1) : 0;
    if (this.phase !== 'paused') this.phase = this.targetIdleFrames > 0 ? 'at_target' : 'running';
    this.pending = null;
    return Object.freeze({ fence: work.offer.fence, frameSequence: evidence.frameSequence, position: this.position,
      travel: copyPoint(travel), groundDistance: actualDistance, totalGroundDistance: total,
      direction: directionOf(travel), yaw: this.yaw, walking: actualWalking, targetIdleFrames: this.targetIdleFrames });
  }
  renderFailed(token: RenderToken): boolean {
    if (!this.pending || token !== this.pending.proposal.token) return false;
    this.stopWork('render_failed'); return true;
  }
  private stopWork(reason: MotionReason | null): void {
    if (this.work) this.terminalOrdinal = Math.max(this.terminalOrdinal, this.work.offer.fence.actionId.ordinal);
    this.work = null; this.pending = null; this.speed = 0; this.targetIdleFrames = 0;
    this.phase = this.binding ? 'idle' : 'unavailable'; this.reason = reason;
    if (this.generation < MAX_COUNTER) this.generation++;
  }
  invalidate(reason: MotionReason = 'disconnected'): void {
    this.stopWork(reason); this.binding = null; this.phase = 'unavailable'; this.sampledAt = null;
  }
  dispose(): void { this.invalidate('disconnected'); this.disposed = true; }
  snapshot(): MotionSnapshot { return Object.freeze({ phase: this.phase, reason: this.reason, position: this.position && copyPoint(this.position), yaw: this.yaw, groundDistance: this.groundDistance, lastSequence: this.lastSequence }); }
}
