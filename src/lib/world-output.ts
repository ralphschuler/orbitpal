// SPDX-License-Identifier: MIT
/** World output protocol v1. Public observations are never native authority.
 * Core owns attachment routes, revisions, clocks, action grants and Store commits.
 * Runtime parsers and the main-window report bridge are implemented separately
 * from the synchronous Avatar render loop. */
import { parseMotionBinding } from './world-motion';
export type Counter = number;
export type PublicId = string;
/** Host-pinned enclosure of current submitted CPU geometry, not raster pixels. */
export const WORLD_ENCLOSURE_KIND = 'conservative_submitted_geometry_v1' as const;
export type EnclosureKind = typeof WORLD_ENCLOSURE_KIND;
export type CssPoint = { x: number; y: number };
export type CssSize = { width: number; height: number };
export type CssRect = { left: number; top: number; right: number; bottom: number };
export type EnvelopeExtents = { left: number; top: number; right: number; bottom: number };
export type LocalReference = { x: number; y: number; z: number };
export type GroundBasis = { right: { x: number; z: number }; down: { x: number; z: number } };
export type UiProof = { connectionId: string; sequence: Counter };
export type ActionId = { actorId: string; worldId: string; ordinal: Counter };
export type RenderReason = 'unregistered' | 'scope_mismatch' | 'stale_binding'
  | 'unsupported_output' | 'unsupported_capability' | 'layout_unconfirmed'
  | 'measurement_unavailable' | 'target_does_not_fit' | 'expired'
  | 'cancelled' | 'storage_unavailable' | 'capacity' | 'protocol_error';
export type BindingStamp = {
  actorId: string; sourceWorldId: string; renderSessionId: PublicId;
  bindingSequence: Counter; worldInstanceId: PublicId; stageId: string;
  configRevision: Counter; topologyRevision: Counter; layoutRevision: Counter;
  assetRevision: Counter; measurementRevision: Counter; referenceRevision: Counter;
};
export type MeasurementChallenge = {
  id: PublicId; expiresInMs: 5000; expected: BindingStamp;
  enclosureKind: EnclosureKind;
  cameraPolicy: 'world_iso_v1'; profileScale: number; effectiveScale: number;
  expectedReferenceLocal: LocalReference | null;
};
export type MotionBinding = BindingStamp & {
  viewport: CssSize; safeCenters: CssRect; maximumFootprint: CssSize;
  envelopeExtents: EnvelopeExtents; referenceLocal: LocalReference;
  position: CssPoint; groundBasis: GroundBasis; modelForwardYaw: number;
  profileScale: number; effectiveScale: number; scale: number;
  reducedMotion: boolean; walkSupported: boolean;
};
/** Native policy wrapper; the reusable pure motion contract stays separate. */
export type NativeMotionBinding = MotionBinding & { enclosureKind: EnclosureKind };
export type RegisteredWorldActor = {
  version: 1; renderSessionId: PublicId; bindingSequence: Counter;
  surfaceId: PublicId; role: 'owner_candidate' | 'mirror';
  readiness:
    | { state: 'awaiting_layout' }
    | { state: 'awaiting_measurement'; challenge: MeasurementChallenge }
    | { state: 'ready'; binding: NativeMotionBinding }
    | { state: 'unavailable'; reason: RenderReason };
};
export type RegisterWorldActor = {
  version: 1; mountId: string; actorId: string;
  worldInstanceId: PublicId; stageId: string; configRevision: Counter;
  studioProof: UiProof | null;
};
export type RenderMeasurement = {
  version: 1; challengeId: PublicId; frameSequence: Counter;
  enclosureKind: EnclosureKind;
  viewport: CssSize; clip: CssRect; anchor: CssPoint; posedBounds: CssRect;
  envelopeExtents: EnvelopeExtents; referenceLocal: LocalReference;
  groundBasis: GroundBasis; modelForwardYaw: number; appliedModelScale: number;
  reducedMotion: boolean; walkSupported: boolean;
};
export type MeasurementAck = {
  version: 1; frameSequence: Counter; output: RegisteredWorldActor;
};
export type MotionFence = {
  actorId: string; renderSessionId: PublicId; bindingSequence: Counter;
  worldInstanceId: PublicId; stageId: string; configRevision: Counter;
  topologyRevision: Counter; layoutRevision: Counter; assetRevision: Counter;
  measurementRevision: Counter; referenceRevision: Counter;
  actionId: ActionId; deliveryId: string;
};
export type FrameObservation = {
  frameSequence: Counter; viewport: CssSize; clip: CssRect;
  enclosureKind: EnclosureKind;
  anchor: CssPoint; posedBounds: CssRect; walking: boolean;
  targetIdleFrames: number; // bounded diagnostic only, never an arrival proof
};
export type ActionReport = {
  version: 1; fence: MotionFence; receiptSequence: Counter;
  frame: FrameObservation;
} & (
  | { kind: 'delivered' }
  | { kind: 'started' }
  | { kind: 'progress' }
  | { kind: 'completed' } // MoveTo only; native derives Evidence::Arrived
);
export type ActionReportAck = {
  version: 1; receiptSequence: Counter;
  outcome: 'accepted' | 'duplicate' | 'cancelled' | 'rejected';
  runtimeStatus: 'awaiting_delivery' | 'delivered' | 'started' | 'paused'
    | 'completed' | 'cancelled' | 'failed' | null;
  confirmedPhase: 'planned' | 'started' | 'completed' | 'cancelled' | null;
  reason: RenderReason | null;
};
export type UnregisterWorldActor = {
  version: 1; renderSessionId: PublicId; bindingSequence: Counter;
};
export type StartLifeMove = { botId: string; target: { x: number; y: number }; ui: UiProof };

/** Adapter-local coordination. These values never grant native authority and
 * are never serialized as extra fields in a native ActionReport. */
export type WorldAdapterIdentity = Readonly<BindingStamp & { challengeId: PublicId | null }>;
export type CanonicalPoint = Readonly<{ x: number; y: number }>;
export type LocalWorldFrame = Readonly<FrameObservation & {
  fence: MotionFence;
  purpose: 'delivery' | 'first_behavior' | 'start_hold' | 'progress';
}>;
export type WorldAdapterReason = RenderReason | 'context_lost' | 'surface_unavailable'
  | 'unsupported_geometry' | 'unsupported_stride' | 'calibration_failed'
  | 'asset_unavailable' | 'render_failed';

export type WorldOutputStatus = { version: 1; botId: string;
  output: RegisteredWorldActor | null; visualAvailable: boolean };
const stampKeys = ['actorId', 'sourceWorldId', 'renderSessionId', 'bindingSequence',
  'worldInstanceId', 'stageId', 'configRevision', 'topologyRevision', 'layoutRevision',
  'assetRevision', 'measurementRevision', 'referenceRevision'] as const;
const reasons: readonly RenderReason[] = ['unregistered', 'scope_mismatch', 'stale_binding',
  'unsupported_output', 'unsupported_capability', 'layout_unconfirmed', 'measurement_unavailable',
  'target_does_not_fit', 'expired', 'cancelled', 'storage_unavailable', 'capacity', 'protocol_error'];
const MAX = Number.MAX_SAFE_INTEGER;
const invalid = () => new Error('Die native Figurenausgabe ist nicht aktuell bestätigt.');
function fields(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw invalid();
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) throw invalid();
  const descriptors = Object.getOwnPropertyDescriptors(value);
  if (Reflect.ownKeys(descriptors).length !== keys.length
    || keys.some(key => !Object.hasOwn(descriptors, key) || !Object.hasOwn(descriptors[key], 'value'))) throw invalid();
  return Object.fromEntries(keys.map(key => [key, descriptors[key].value]));
}
function finite(value: unknown, min: number, max: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max) throw invalid();
  return value;
}
function count(value: unknown, min = 1, max = MAX): number {
  const n = finite(value, min, max); if (!Number.isSafeInteger(n)) throw invalid(); return n;
}
function id(value: unknown): string {
  if (typeof value !== 'string' || !/^[A-Za-z0-9_-]{1,96}$/.test(value)) throw invalid(); return value;
}
function oneOf<T extends string>(value: unknown, choices: readonly T[]): T {
  if (typeof value !== 'string' || !choices.includes(value as T)) throw invalid(); return value as T;
}
function stamp(value: unknown): BindingStamp {
  const data = fields(value, stampKeys);
  for (const key of ['actorId', 'sourceWorldId', 'renderSessionId', 'worldInstanceId', 'stageId']) id(data[key]);
  for (const key of ['bindingSequence', 'configRevision', 'topologyRevision', 'layoutRevision',
    'assetRevision', 'measurementRevision', 'referenceRevision']) count(data[key]);
  return Object.freeze(data) as BindingStamp;
}
function reference(value: unknown): LocalReference {
  const data = fields(value, ['x', 'y', 'z']);
  return Object.freeze({ x: finite(data.x, -1e6, 1e6), y: finite(data.y, -1e6, 1e6), z: finite(data.z, -1e6, 1e6) });
}
export function parseMeasurementChallenge(value: unknown): MeasurementChallenge {
  const data = fields(value, ['id', 'expiresInMs', 'expected', 'enclosureKind', 'cameraPolicy', 'profileScale', 'effectiveScale', 'expectedReferenceLocal']);
  if (data.expiresInMs !== 5000 || data.cameraPolicy !== 'world_iso_v1' || data.enclosureKind !== WORLD_ENCLOSURE_KIND) throw invalid();
  return Object.freeze({ id: id(data.id), expiresInMs: 5000, expected: stamp(data.expected), cameraPolicy: 'world_iso_v1',
    enclosureKind: WORLD_ENCLOSURE_KIND,
    profileScale: finite(data.profileScale, .3, 2.5), effectiveScale: finite(data.effectiveScale, .3, 2.5),
    expectedReferenceLocal: data.expectedReferenceLocal === null ? null : reference(data.expectedReferenceLocal) });
}
/** Validate the native policy before passing the separate pure motion contract
 * to its existing parser. Missing/unknown kinds never acquire a default. */
export function parseWorldMotionBinding(value: unknown): NativeMotionBinding | null {
  try {
    const data = fields(value, [...stampKeys, 'enclosureKind', 'viewport', 'safeCenters', 'maximumFootprint',
      'envelopeExtents', 'referenceLocal', 'position', 'groundBasis', 'modelForwardYaw',
      'profileScale', 'effectiveScale', 'scale', 'reducedMotion', 'walkSupported']);
    if (data.enclosureKind !== WORLD_ENCLOSURE_KIND) return null;
    const { enclosureKind: _kind, ...motion } = data;
    const binding = parseMotionBinding(motion);
    return binding ? Object.freeze({ ...binding, enclosureKind: WORLD_ENCLOSURE_KIND }) : null;
  } catch { return null; }
}
/** Exact closed wire shapes; accessors/prototype properties never become grants. */
export function parseRegisteredWorldActor(value: unknown): RegisteredWorldActor {
  const data = fields(value, ['version', 'renderSessionId', 'bindingSequence', 'surfaceId', 'role', 'readiness']);
  if (data.version !== 1) throw invalid();
  const raw = data.readiness;
  if (!raw || typeof raw !== 'object') throw invalid();
  const descriptor = Object.getOwnPropertyDescriptor(raw, 'state');
  if (!descriptor || !Object.hasOwn(descriptor, 'value')) throw invalid();
  let readiness: RegisteredWorldActor['readiness'];
  switch (descriptor.value) {
    case 'awaiting_layout': fields(raw, ['state']); readiness = { state: 'awaiting_layout' }; break;
    case 'awaiting_measurement': {
      const inner = fields(raw, ['state', 'challenge']);
      readiness = { state: 'awaiting_measurement', challenge: parseMeasurementChallenge(inner.challenge) }; break;
    }
    case 'ready': {
      const inner = fields(raw, ['state', 'binding']), binding = parseWorldMotionBinding(inner.binding);
      if (!binding) throw invalid(); readiness = { state: 'ready', binding }; break;
    }
    case 'unavailable': {
      const inner = fields(raw, ['state', 'reason']); readiness = { state: 'unavailable', reason: oneOf(inner.reason, reasons) }; break;
    }
    default: throw invalid();
  }
  const renderSessionId = id(data.renderSessionId), bindingSequence = count(data.bindingSequence);
  const expected = readiness.state === 'ready' ? readiness.binding : readiness.state === 'awaiting_measurement' ? readiness.challenge.expected : null;
  if (expected && (expected.renderSessionId !== renderSessionId || expected.bindingSequence !== bindingSequence)) throw invalid();
  return Object.freeze({ version: 1, renderSessionId, bindingSequence, surfaceId: id(data.surfaceId),
    role: oneOf(data.role, ['owner_candidate', 'mirror'] as const), readiness: Object.freeze(readiness) });
}
export function parseMeasurementAck(value: unknown, frameSequence: number): MeasurementAck {
  const data = fields(value, ['version', 'frameSequence', 'output']);
  if (data.version !== 1 || count(data.frameSequence) !== frameSequence) throw invalid();
  return Object.freeze({ version: 1, frameSequence, output: parseRegisteredWorldActor(data.output) });
}
export function parseActionReportAck(value: unknown, receiptSequence: number): ActionReportAck {
  const data = fields(value, ['version', 'receiptSequence', 'outcome', 'runtimeStatus', 'confirmedPhase', 'reason']);
  if (data.version !== 1 || count(data.receiptSequence) !== receiptSequence) throw invalid();
  return Object.freeze({ version: 1, receiptSequence,
    outcome: oneOf(data.outcome, ['accepted', 'duplicate', 'cancelled', 'rejected'] as const),
    runtimeStatus: data.runtimeStatus === null ? null : oneOf(data.runtimeStatus, ['awaiting_delivery', 'delivered', 'started', 'paused', 'completed', 'cancelled', 'failed'] as const),
    confirmedPhase: data.confirmedPhase === null ? null : oneOf(data.confirmedPhase, ['planned', 'started', 'completed', 'cancelled'] as const),
    reason: data.reason === null ? null : oneOf(data.reason, reasons) });
}
export function parseWorldOutputStatus(value: unknown, botId: string): WorldOutputStatus {
  const data = fields(value, ['version', 'botId', 'output', 'visualAvailable']);
  if (data.version !== 1 || id(data.botId) !== id(botId) || typeof data.visualAvailable !== 'boolean') throw invalid();
  const output = data.output === null ? null : parseRegisteredWorldActor(data.output);
  const expected = output?.readiness.state === 'ready' ? output.readiness.binding
    : output?.readiness.state === 'awaiting_measurement' ? output.readiness.challenge.expected : null;
  if (expected && expected.actorId !== botId) throw invalid();
  if (data.visualAvailable && (!output || output.role !== 'owner_candidate' || output.readiness.state !== 'ready')) throw invalid();
  return Object.freeze({ version: 1, botId, output, visualAvailable: data.visualAvailable });
}
