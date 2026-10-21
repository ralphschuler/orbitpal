import type { EmotePayload } from './presentation';

export type LifeMode = 'ambient' | 'life';
export const lifeReasons = {
  ready: 'Bereit für selbstständige Beiträge.',
  model_unavailable: 'Für Sprache ein Dialogmodell auswählen. Life kann lokal weiterlaufen.',
  ui_unavailable: 'Studio-Verbindung nicht bestätigt. Selbstständige Anfragen sind gesperrt.',
  ui_blocked: 'Wartet auf das Ende deiner Eingabe oder einer laufenden Ausgabe.',
  system_unavailable: 'Bildschirmsperre oder Systemverfügbarkeit nicht freigegeben.',
  output_unavailable: 'Studioansicht oder eine aktive Figurenausgabe auswählen.',
  storage_unavailable: 'Der lokale Life-Zustand ist nicht verfügbar. Life pausiert.',
} as const;
export const systemReasons = {
  ready: 'System ist wach; die Sitzung ist als entsperrt gemeldet.',
  initializing: 'Systemstatus wird erneut geprüft.',
  locked: 'Bildschirm ist gesperrt. Selbstständige Anfragen pausieren.',
  sleeping: 'Computer schläft. Selbstständige Anfragen pausieren.',
  inactive_session: 'Die aktuelle Sitzung ist nicht aktiv.',
  protected_data_unavailable: 'Geschützte Systemdaten sind nicht verfügbar.',
  observation_unavailable: 'Der Sperrstatus ist nicht verlässlich verfügbar. Selbstständige Anfragen sind gesperrt.',
  monitor_disconnected: 'Die Systemüberwachung ist getrennt. Selbstständige Anfragen sind gesperrt.',
  unsupported: 'Dieses System unterstützt die native Verfügbarkeitsprüfung nicht.',
} as const;
export const activityLabels = { move_to: 'Unterwegs', rest: 'Ruhen', recharge: 'Aufladen', play: 'Spielen', snack: 'Snackpause', socialize: 'Gemeinsame Zeit', look_at: 'Umschauen', wave: 'Winken' } as const;
export const phaseLabels = { planned: 'Geplant', started: 'Gestartet', completed: 'Abgeschlossen', cancelled: 'Abgebrochen' } as const;
type LifeReason = keyof typeof lifeReasons;
type SystemReason = keyof typeof systemReasons;
export interface PublicActor {
  schemaVersion: 1; actorId: string; worldId: string; revision: number; simulationTime: number;
  activity: { id: { actorId: string; worldId: string; ordinal: number }; activity: keyof typeof activityLabels; phase: keyof typeof phaseLabels } | null;
}
export interface LifeSession {
  botId: string; mode: LifeMode; sessionId: string; revision: number; active: boolean; paused: boolean; reason: LifeReason;
  speech: { eligible: boolean; inFlight: boolean; turnId: string | null; remainingSeconds: number; attempts: number; limit: number; budgetLimited: boolean; waitingForBot: string | null };
  actor?: PublicActor;
}
export interface LifeSnapshot {
  version: 1; sequence: number;
  system: { state: 'awake_unlocked' | 'suspended_locked' | 'unknown'; reason: SystemReason; generation: number; source: 'macos_cg_session_compatibility' | 'windows_wts' | 'linux_logind' | 'unsupported' };
  sessions: LifeSession[];
}
export type LifeBot = { availability: 'ready'; actor: PublicActor | null } | { availability: 'not_initialized' | 'unavailable' };
export interface LifeReply {
  id: string; botId: string; skipped?: boolean; text: string; speechText?: string;
  voice?: { provider: string; voiceId: string | null; language: string; enabled: boolean; rate: number; volume: number; pitch: number };
  presentationOutput?:'studio'|'overlay'|'obs';presentationRouteId?:string|null;
  emotion?: string; emotes: EmotePayload[]; source: string; user: string;
}
export interface LifeTurn { kind: 'started' | 'reply' | 'finished'; botId: string; sessionId: string; revision: number; turnId: string; reply: LifeReply | null; }
const invalid = () => new Error('Die native Life-Laufzeit hat ungültige Daten geliefert. Selbstständige Ausgaben bleiben gesperrt.');
function object(value: unknown): Record<string, unknown> { if (!value || typeof value !== 'object' || Array.isArray(value)) throw invalid(); return value as Record<string, unknown>; }
function count(value: unknown, min = 0, max = Number.MAX_SAFE_INTEGER): number { if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < min || value > max) throw invalid(); return value; }
function flag(value: unknown): boolean { if (typeof value !== 'boolean') throw invalid(); return value; }
function text(value: unknown, max: number, allowEmpty = true): string { if (typeof value !== 'string' || value.length > max || !allowEmpty && !value.length) throw invalid(); return value; }
function id(value: unknown): string { const result = text(value, 96, false); if (!/^[A-Za-z0-9_-]+$/.test(result)) throw invalid(); return result; }
function token(value: unknown): string { const result = text(value, 128, false); if (/[\u0000-\u001f\u007f]/.test(result)) throw invalid(); return result; }
function choice<T extends string>(value: unknown, values: readonly T[]): T { if (typeof value !== 'string' || !values.includes(value as T)) throw invalid(); return value as T; }
function key<T extends string>(value: unknown, values: Record<T, string>): T { return choice(value, Object.keys(values) as T[]); }
function finite(value: unknown, min: number, max: number): number { if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max) throw invalid(); return value; }
export function publicActor(value: unknown): PublicActor {
  const item = object(value); if (item.schemaVersion !== 1) throw invalid();
  const actorId = id(item.actorId), worldId = id(item.worldId);
  let activity: PublicActor['activity'] = null;
  if (item.activity !== null) {
    const current = object(item.activity), action = object(current.id);
    if (id(action.actorId) !== actorId || id(action.worldId) !== worldId) throw invalid();
    activity = { id: { actorId, worldId, ordinal: count(action.ordinal, 1) }, activity: key(current.activity, activityLabels), phase: key(current.phase, phaseLabels) };
  }
  return { schemaVersion: 1, actorId, worldId, revision: count(item.revision), simulationTime: count(item.simulationTime), activity };
}
export function parseLifeBot(value: unknown): LifeBot {
  const item = object(value), availability = choice(item.availability, ['ready', 'not_initialized', 'unavailable']);
  return availability === 'ready' ? { availability, actor: item.actor === null ? null : publicActor(item.actor) } : { availability };
}
export function parseLifeSnapshot(value: unknown): LifeSnapshot {
  const item = object(value), system = object(item.system);
  if (item.version !== 1 || !Array.isArray(item.sessions) || item.sessions.length > 64) throw invalid();
  const state = choice(system.state, ['awake_unlocked', 'suspended_locked', 'unknown']), reason = key(system.reason, systemReasons);
  if ((state === 'awake_unlocked') !== (reason === 'ready') || state === 'suspended_locked' && !['locked', 'sleeping', 'inactive_session', 'protected_data_unavailable'].includes(reason)) throw invalid();
  const seen = new Set<string>();
  const sessions = item.sessions.map(value => {
    const session = object(value), speech = object(session.speech), botId = id(session.botId);
    if (seen.has(botId)) throw invalid(); seen.add(botId);
    const actor = session.actor === undefined ? undefined : publicActor(session.actor);
    if (actor && actor.actorId !== botId) throw invalid();
    const inFlight = flag(speech.inFlight), turnId = speech.turnId === null ? null : token(speech.turnId);
    if (inFlight !== (turnId !== null)) throw invalid();
    return { botId, mode: choice(session.mode, ['ambient', 'life']), sessionId: token(session.sessionId), revision: count(session.revision), active: flag(session.active), paused: flag(session.paused), reason: key(session.reason, lifeReasons),
      speech: { eligible: flag(speech.eligible), inFlight, turnId, remainingSeconds: count(speech.remainingSeconds), attempts: count(speech.attempts), limit: count(speech.limit, 1, 120), budgetLimited: flag(speech.budgetLimited), waitingForBot: speech.waitingForBot === null ? null : id(speech.waitingForBot) }, ...(actor ? { actor } : {}) };
  });
  return { version: 1, sequence: count(item.sequence), system: { state, reason, generation: count(system.generation, 1), source: choice(system.source, ['macos_cg_session_compatibility', 'windows_wts', 'linux_logind', 'unsupported']) }, sessions };
}
function reply(value: unknown, botId: string, turnId: string): LifeReply {
  const item = object(value);
  if (item.skipped === true) return { id: turnId, botId, skipped: true, text: '', emotes: [], source: 'autopilot', user: 'Autopilot' };
  if (item.id !== turnId || item.botId !== botId || !Array.isArray(item.emotes) || item.emotes.length > 64) throw invalid();
  const voice = object(item.voice);
  const presentationOutput=item.presentationOutput===undefined?undefined:choice(item.presentationOutput,['studio','overlay','obs']);
  const presentationRouteId=item.presentationRouteId===undefined?undefined:item.presentationRouteId===null?null:token(item.presentationRouteId);
  if(presentationRouteId&&presentationOutput!=='overlay')throw invalid();
  const emotes: EmotePayload[] = item.emotes.map(value => {
    const emote = object(value), result: EmotePayload = { action: text(emote.action, 512, false) };
    for (const name of ['id', 'sound', 'bufferSound'] as const) if (emote[name] !== undefined) result[name] = text(emote[name], 512);
    if (emote.nonce !== undefined) result.nonce = typeof emote.nonce === 'number' ? count(emote.nonce) : token(emote.nonce);
    for (const name of ['duration', 'intensity', 'soundVolume', 'soundOffset', 'soundRate'] as const) if (emote[name] !== undefined) result[name] = finite(emote[name], 0, 3600);
    return result;
  });
  return { id: turnId, botId, ...(presentationOutput?{presentationOutput}:{}), ...(presentationRouteId===undefined?{}:{presentationRouteId}), text: text(item.text, 200_000), ...(item.speechText === undefined ? {} : { speechText: text(item.speechText, 200_000) }),
    voice: { provider: text(voice.provider, 64, false), voiceId: voice.voiceId === null ? null : text(voice.voiceId, 2048), language: text(voice.language, 64), enabled: flag(voice.enabled), rate: finite(voice.rate, .5, 2), volume: finite(voice.volume, 0, 1), pitch: finite(voice.pitch, .5, 2) },
    emotion: text(item.emotion ?? 'neutral', 256), emotes, source: text(item.source ?? 'autopilot', 64), user: text(item.user ?? 'Autopilot', 256) };
}
export function parseLifeTurn(value: unknown): LifeTurn {
  const item = object(value), kind = choice(item.kind, ['started', 'reply', 'finished']), botId = id(item.botId), turnId = token(item.turnId);
  return { kind, botId, sessionId: token(item.sessionId), revision: count(item.revision), turnId, reply: kind === 'reply' ? reply(item.reply, botId, turnId) : null };
}
export function nativeSchedule(session: LifeSession | undefined) {
  return session ? { ...session.speech, waitingForBot: session.speech.waitingForBot ?? undefined } : undefined;
}
export function currentLifeTurn(turn: LifeTurn, snapshot: LifeSnapshot | undefined): boolean {
  return !!snapshot?.sessions.some(session => session.active && session.botId === turn.botId && session.sessionId === turn.sessionId && session.revision === turn.revision && session.speech.turnId === turn.turnId);
}
export function sameLifeTurn(left: LifeTurn | undefined, right: LifeTurn): boolean {
  return !!left && left.botId === right.botId && left.sessionId === right.sessionId && left.revision === right.revision && left.turnId === right.turnId;
}

/** Read-only native ordering. Equal sequence snapshots can contain a newer
 * native countdown; they never schedule or grant a renderer turn. */
export class LifeSnapshots {
  private sequence = -1;
  accept(value: unknown): LifeSnapshot | undefined { const next = parseLifeSnapshot(value); if (next.sequence < this.sequence) return; this.sequence = next.sequence; return next; }
}

/** A reply may overtake the asynchronous fresh-snapshot check for its start.
 * Wait only for that exact admission; never queue cancellation behind audio. */
export class LifeTurnAdmissions {
  private accepted?: LifeTurn;
  private candidates = new Map<string, { turn: LifeTurn; ready: Promise<boolean> }>();
  private key(turn: LifeTurn): string { return JSON.stringify([turn.botId, turn.sessionId, turn.revision, turn.turnId]); }
  start(turn: LifeTurn, admission: (current: () => boolean) => Promise<boolean>): Promise<boolean> {
    if (sameLifeTurn(this.accepted, turn)) return Promise.resolve(true);
    const key = this.key(turn), previous = this.candidates.get(key);
    if (previous) return previous.ready;
    // A delayed/rejected candidate cannot evict an already accepted turn, nor
    // can a burst of host events allocate an unbounded set of pending checks.
    if (this.candidates.size >= 16) return Promise.resolve(false);
    const record = { turn, ready: Promise.resolve(false) };
    this.candidates.set(key, record);
    const current = () => this.candidates.get(key) === record;
    record.ready = Promise.resolve().then(() => current() ? admission(current) : false).then(allowed => {
      if (!allowed || !current()) return false;
      this.accepted = turn;
      this.candidates.clear();
      return true;
    }, () => false).finally(() => { if (current()) this.candidates.delete(key); });
    return record.ready;
  }
  async ready(turn: LifeTurn): Promise<boolean> {
    if (sameLifeTurn(this.accepted, turn)) return true;
    const record = this.candidates.get(this.key(turn));
    return !!record && await record.ready && sameLifeTurn(this.accepted, turn);
  }
  finish(turn: LifeTurn): void {
    this.candidates.delete(this.key(turn));
    if (sameLifeTurn(this.accepted, turn)) this.accepted = undefined;
  }
  cancel(botId?: string): void {
    for (const [key, record] of this.candidates) if (!botId || record.turn.botId === botId) this.candidates.delete(key);
    if (!botId || this.accepted?.botId === botId) this.accepted = undefined;
  }
}
