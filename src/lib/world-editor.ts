import { canonicalToPercent, percentToCanonical, type PointDraft, type WorldDraft, type WorldPoint, type WorldRegion, type WorldSnapshot } from './world-state';

/** Local editing geometry only. These values never grant runtime/output authority. */
export const EDITOR_EDGE = 1_000_000;
export const EDITOR_STEP = 10_000;
export type EditorCorner = 'nw' | 'ne' | 'sw' | 'se';
export type EditorTarget =
  | { kind: 'actor'; id: string }
  | { kind: 'anchor'; id: string }
  | { kind: 'slot'; anchorId: string; id: string }
  | { kind: 'boundary'; corner: EditorCorner };
export type ActorPositions = Record<string, WorldPoint>;
export interface EditorModel { draft: WorldDraft; positions: ActorPositions }
export interface CanvasRect { left: number; top: number; width: number; height: number }

const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value));
export function readEditorPoint(point: PointDraft): WorldPoint {
  return { x: percentToCanonical(point.x), y: percentToCanonical(point.y) };
}
export function writeEditorPoint(point: WorldPoint): PointDraft {
  return { x: canonicalToPercent(point.x), y: canonicalToPercent(point.y) };
}
export function editorRegion(draft: WorldDraft): WorldRegion {
  return { min: readEditorPoint(draft.safeRegion.min), max: readEditorPoint(draft.safeRegion.max) };
}
export function editorCenter(region: WorldRegion): WorldPoint {
  return { x: Math.round((region.min.x + region.max.x) / 2), y: Math.round((region.min.y + region.max.y) / 2) };
}
export function editorSpawn(region: WorldRegion, index: number): WorldPoint {
  const cell = Math.max(0, Math.trunc(index)) % 16;
  return { x: Math.round(region.min.x + (region.max.x - region.min.x) * (1 + cell % 4) / 5),
    y: Math.round(region.min.y + (region.max.y - region.min.y) * (1 + Math.floor(cell / 4)) / 5) };
}
export function containsEditorPoint(region: WorldRegion, point: WorldPoint): boolean {
  return Number.isSafeInteger(point.x) && Number.isSafeInteger(point.y)
    && point.x >= region.min.x && point.x <= region.max.x && point.y >= region.min.y && point.y <= region.max.y;
}
export function editorActors(draft: WorldDraft, snapshot: WorldSnapshot | undefined, positions: ActorPositions): ActorPositions {
  const center = editorCenter(editorRegion(draft));
  const confirmed = snapshot?.stageId === draft.stageId ? snapshot : undefined;
  return Object.fromEntries(draft.members.map(id => [id, Object.hasOwn(positions, id) ? positions[id]
    : confirmed?.actors.find(actor => actor.actorId === id)?.position ?? { ...center }]));
}
export function canvasPoint(x: number, y: number, rect: CanvasRect): WorldPoint | null {
  if (![x, y, rect.left, rect.top, rect.width, rect.height].every(Number.isFinite) || rect.width <= 0 || rect.height <= 0) return null;
  return { x: Math.round(clamp((x - rect.left) / rect.width, 0, 1) * EDITOR_EDGE), y: Math.round(clamp((y - rect.top) / rect.height, 0, 1) * EDITOR_EDGE) };
}
export function targetPoint(model: EditorModel, target: EditorTarget, actors: ActorPositions): WorldPoint | null {
  if (target.kind === 'boundary') {
    const region = editorRegion(model.draft);
    return { x: target.corner.endsWith('w') ? region.min.x : region.max.x, y: target.corner.startsWith('n') ? region.min.y : region.max.y };
  }
  if (target.kind === 'actor') return model.draft.members.includes(target.id) && Object.hasOwn(actors, target.id) ? actors[target.id] : null;
  const anchor = model.draft.anchors.find(item => item.id === (target.kind === 'anchor' ? target.id : target.anchorId));
  if (!anchor) return null;
  const point = target.kind === 'anchor' ? anchor.position : anchor.slots.find(slot => slot.id === target.id)?.position;
  return point ? readEditorPoint(point) : null;
}
export function targetKey(target: EditorTarget | null): string {
  if (!target) return '';
  return target.kind === 'boundary' ? `boundary:${target.corner}` : target.kind === 'slot' ? `slot:${target.anchorId}:${target.id}` : `${target.kind}:${target.id}`;
}
export function cloneEditorModel(model: EditorModel): EditorModel {
  return {
    draft: { ...model.draft, members: [...model.draft.members],
      safeRegion: { min: { ...model.draft.safeRegion.min }, max: { ...model.draft.safeRegion.max } },
      anchors: model.draft.anchors.map(anchor => ({ ...anchor, position: { ...anchor.position }, slots: anchor.slots.map(slot => ({ ...slot, position: { ...slot.position } })) })) },
    positions: Object.fromEntries(Object.entries(model.positions).map(([id, point]) => [id, { ...point }])),
  };
}

/** A boundary never strands an existing anchor, seat or displayed actor. */
function resizeBoundary(model: EditorModel, corner: EditorCorner, point: WorldPoint, actors: ActorPositions): EditorModel {
  const next = cloneEditorModel(model), region = editorRegion(model.draft);
  const points = [...Object.values(actors), ...model.draft.anchors.flatMap(anchor => [readEditorPoint(anchor.position), ...anchor.slots.map(slot => readEditorPoint(slot.position))])];
  const hull = points.length ? {
    min: { x: Math.min(...points.map(p => p.x)), y: Math.min(...points.map(p => p.y)) },
    max: { x: Math.max(...points.map(p => p.x)), y: Math.max(...points.map(p => p.y)) },
  } : { min: { ...region.max }, max: { ...region.min } };
  // Existing valid narrow imported regions can expand without being silently rewritten.
  const gapX = Math.min(EDITOR_STEP, region.max.x - region.min.x), gapY = Math.min(EDITOR_STEP, region.max.y - region.min.y);
  if (corner.endsWith('w')) region.min.x = clamp(point.x, 0, Math.min(region.max.x - gapX, hull.min.x));
  else region.max.x = clamp(point.x, Math.max(region.min.x + gapX, hull.max.x), EDITOR_EDGE);
  if (corner.startsWith('n')) region.min.y = clamp(point.y, 0, Math.min(region.max.y - gapY, hull.min.y));
  else region.max.y = clamp(point.y, Math.max(region.min.y + gapY, hull.max.y), EDITOR_EDGE);
  next.draft.safeRegion = { min: writeEditorPoint(region.min), max: writeEditorPoint(region.max) };
  return next;
}

export function moveEditorTarget(model: EditorModel, target: EditorTarget, point: WorldPoint, actors: ActorPositions): EditorModel {
  if (![point.x, point.y].every(Number.isSafeInteger)) return model;
  if (target.kind === 'boundary') return resizeBoundary(model, target.corner, point, actors);
  const current = targetPoint(model, target, actors);
  if (!current) return model;
  const region = editorRegion(model.draft);
  const position = { x: clamp(point.x, region.min.x, region.max.x), y: clamp(point.y, region.min.y, region.max.y) };
  const next = cloneEditorModel(model);
  if (target.kind === 'actor') {
    next.positions = { ...next.positions, [target.id]: position };
  } else if (target.kind === 'slot') {
    const slot = next.draft.anchors.find(anchor => anchor.id === target.anchorId)?.slots.find(slot => slot.id === target.id);
    if (slot) slot.position = writeEditorPoint(position);
  } else {
    const anchor = next.draft.anchors.find(anchor => anchor.id === target.id)!;
    const points = [readEditorPoint(anchor.position), ...anchor.slots.map(slot => readEditorPoint(slot.position))];
    const dx = clamp(point.x - current.x, region.min.x - Math.min(...points.map(p => p.x)), region.max.x - Math.max(...points.map(p => p.x)));
    const dy = clamp(point.y - current.y, region.min.y - Math.min(...points.map(p => p.y)), region.max.y - Math.max(...points.map(p => p.y)));
    anchor.position = writeEditorPoint({ x: current.x + dx, y: current.y + dy });
    anchor.slots = anchor.slots.map(slot => { const p = readEditorPoint(slot.position); return { ...slot, position: writeEditorPoint({ x: p.x + dx, y: p.y + dy }) }; });
  }
  return next;
}
export function nudgeEditorTarget(model: EditorModel, target: EditorTarget, key: string, coarse: boolean, actors: ActorPositions): EditorModel {
  const current = targetPoint(model, target, actors);
  if (!current || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(key)) return model;
  const step = EDITOR_STEP * (coarse ? 5 : 1);
  return moveEditorTarget(model, target, { x: current.x + (key === 'ArrowRight' ? step : key === 'ArrowLeft' ? -step : 0), y: current.y + (key === 'ArrowDown' ? step : key === 'ArrowUp' ? -step : 0) }, actors);
}
export function removeEditorTarget(model: EditorModel, target: EditorTarget): EditorModel {
  const next = cloneEditorModel(model);
  if (target.kind === 'actor') {
    next.draft.members = next.draft.members.filter(id => id !== target.id);
    next.positions = Object.fromEntries(Object.entries(next.positions).filter(([id]) => id !== target.id));
  } else if (target.kind === 'anchor') next.draft.anchors = next.draft.anchors.filter(anchor => anchor.id !== target.id);
  else if (target.kind === 'slot') {
    const anchor = next.draft.anchors.find(anchor => anchor.id === target.anchorId);
    if (anchor && anchor.slots.length > 1) anchor.slots = anchor.slots.filter(slot => slot.id !== target.id);
  }
  return next;
}
