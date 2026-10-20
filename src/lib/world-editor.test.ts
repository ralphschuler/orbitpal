import { describe, expect, it } from 'vitest';
import { draftWorld, newWorldDraft, type WorldSnapshot } from './world-state';
import { canvasPoint, cloneEditorModel, containsEditorPoint, editorActors, editorRegion, editorSpawn, moveEditorTarget, nudgeEditorTarget, readEditorPoint, removeEditorTarget, type EditorModel } from './world-editor';

function fixture(): EditorModel {
  const draft = newWorldDraft('stage_test'); draft.members = ['nova', 'james'];
  draft.anchors = [{ id: 'rest', kind: 'rest', position: { x: '50', y: '50' }, slots: [
    { id: 'seat1', position: { x: '60', y: '60' } }, { id: 'seat2', position: { x: '40', y: '60' } },
  ] }];
  return { draft, positions: {} };
}
const actors = { nova: { x: 200_000, y: 720_000 }, james: { x: 500_000, y: 500_000 } };

describe('visual world editing geometry', () => {
  it('maps CSS viewport coordinates without using physical DPI and rejects invalid surfaces', () => {
    expect(canvasPoint(300, 250, { left: 100, top: 50, width: 400, height: 400 })).toEqual({ x: 500_000, y: 500_000 });
    expect(canvasPoint(-50, 900, { left: 100, top: 50, width: 400, height: 400 })).toEqual({ x: 0, y: 1_000_000 });
    expect(canvasPoint(1, 1, { left: 0, top: 0, width: 0, height: 4 })).toBeNull();
    expect(canvasPoint(Number.NaN, 1, { left: 0, top: 0, width: 4, height: 4 })).toBeNull();
  });
  it('moves an entire object and its seats by one constrained translation without mutating the source', () => {
    const original = fixture(), before = structuredClone(original);
    const next = moveEditorTarget(original, { kind: 'anchor', id: 'rest' }, { x: 1_000_000, y: 1_000_000 }, actors);
    expect(readEditorPoint(next.draft.anchors[0].position)).toEqual({ x: 850_000, y: 850_000 });
    expect(next.draft.anchors[0].slots.map(slot => readEditorPoint(slot.position))).toEqual([{ x: 950_000, y: 950_000 }, { x: 750_000, y: 950_000 }]);
    expect(next.draft.anchors[0].slots.map(slot => slot.id)).toEqual(['seat1', 'seat2']);
    expect(original).toEqual(before); expect(() => draftWorld(next.draft)).not.toThrow();
  });
  it('moves one seat independently and clamps it to the movement boundary', () => {
    const next = moveEditorTarget(fixture(), { kind: 'slot', anchorId: 'rest', id: 'seat1' }, { x: -1, y: 1_500_000 }, actors);
    expect(readEditorPoint(next.draft.anchors[0].slots[0].position)).toEqual({ x: 50_000, y: 950_000 });
    expect(next.draft.anchors[0].position).toEqual({ x: '50', y: '50' });
    expect(next.draft.anchors[0].slots[1].position).toEqual({ x: '40', y: '60' });
  });
  it('does not let resized boundaries strand an actor or a seat', () => {
    const next = moveEditorTarget(fixture(), { kind: 'boundary', corner: 'nw' }, { x: 900_000, y: 900_000 }, actors);
    const region = editorRegion(next.draft);
    expect(region.min).toEqual({ x: 200_000, y: 500_000 });
    for (const point of Object.values(actors)) expect(containsEditorPoint(region, point)).toBe(true);
    for (const anchor of next.draft.anchors) for (const seat of anchor.slots) expect(containsEditorPoint(region, readEditorPoint(seat.position))).toBe(true);
    expect(() => draftWorld(next.draft)).not.toThrow();
  });
  it('prevents crossed or collapsed empty boundaries and preserves valid imported narrow regions', () => {
    const base = fixture(); base.draft.members = []; base.draft.anchors = [];
    const next = moveEditorTarget(base, { kind: 'boundary', corner: 'se' }, { x: 0, y: 0 }, {});
    expect(editorRegion(next.draft)).toEqual({ min: { x: 50_000, y: 50_000 }, max: { x: 60_000, y: 60_000 } });
    expect(() => draftWorld(next.draft)).not.toThrow();
    base.draft.safeRegion = { min: { x: '0', y: '0' }, max: { x: '0.001', y: '0.001' } };
    expect(() => draftWorld(moveEditorTarget(base, { kind: 'boundary', corner: 'se' }, { x: 0, y: 0 }, {}).draft)).not.toThrow();
  });
  it('uses fine and Shift coarse keyboard steps for only the selected actor', () => {
    const original = fixture();
    const fine = nudgeEditorTarget(original, { kind: 'actor', id: 'nova' }, 'ArrowRight', false, actors);
    const coarse = nudgeEditorTarget(original, { kind: 'actor', id: 'nova' }, 'ArrowUp', true, actors);
    expect(fine.positions).toEqual({ nova: { x: 210_000, y: 720_000 } });
    expect(coarse.positions).toEqual({ nova: { x: 200_000, y: 670_000 } });
    expect(original.positions).toEqual({}); expect(original.draft.members).toEqual(['nova', 'james']);
  });
  it('rejects nonfinite movement and does not create positions for nonparticipants', () => {
    const original = fixture();
    expect(moveEditorTarget(original, { kind: 'actor', id: 'nova' }, { x: Number.NaN, y: 4 }, actors)).toBe(original);
    expect(moveEditorTarget(original, { kind: 'actor', id: 'foreign' }, { x: 4, y: 4 }, actors)).toBe(original);
  });
  it('removing one actor discards only that actor placement draft', () => {
    const original = fixture(); original.positions = structuredClone(actors);
    const next = removeEditorTarget(original, { kind: 'actor', id: 'nova' });
    expect(next.draft.members).toEqual(['james']); expect(next.positions).toEqual({ james: actors.james });
    expect(original.draft.members).toEqual(['nova', 'james']); expect(original.positions).toEqual(actors);
  });
  it('keeps at least one seat per object, while deleting an object removes its own seats', () => {
    const first = removeEditorTarget(fixture(), { kind: 'slot', anchorId: 'rest', id: 'seat1' });
    const last = removeEditorTarget(first, { kind: 'slot', anchorId: 'rest', id: 'seat2' });
    expect(last.draft.anchors[0].slots.map(slot => slot.id)).toEqual(['seat2']);
    expect(removeEditorTarget(last, { kind: 'anchor', id: 'rest' }).draft.anchors).toEqual([]);
    expect(() => draftWorld(last.draft)).not.toThrow();
  });
  it('keeps confirmed overlapping positions unchanged and ignores a different stage lineage', () => {
    const model = fixture(), snapshot: WorldSnapshot = { schemaVersion: 1, stageId: 'stage_test', configRevision: 2, sequence: 1,
      safeRegion: editorRegion(model.draft), anchors: [], occupied: [], actors: [
        { actorId: 'nova', position: { x: 500_000, y: 500_000 }, life: null }, { actorId: 'james', position: { x: 500_000, y: 500_000 }, life: null },
      ] };
    expect(editorActors(model.draft, snapshot, {})).toEqual({ nova: { x: 500_000, y: 500_000 }, james: { x: 500_000, y: 500_000 } });
    snapshot.stageId = 'other_stage'; snapshot.actors[0].position = { x: 900_000, y: 900_000 };
    expect(editorActors(model.draft, snapshot, {}).nova).toEqual({ x: 500_000, y: 500_000 });
  });
  it('spreads new local handles deterministically and keeps every suggested point inside the region', () => {
    const region = editorRegion(fixture().draft);
    expect(editorSpawn(region, 0)).not.toEqual(editorSpawn(region, 1));
    for (let i = 0; i < 64; i++) expect(containsEditorPoint(region, editorSpawn(region, i))).toBe(true);
    expect(editorSpawn(region, 3)).toEqual(editorSpawn(region, 3));
  });
  it('supports prototype-like validated actor ids without reading inherited object properties', () => {
    const model = fixture(); model.draft.members = ['__proto__'];
    const next = moveEditorTarget(model, { kind: 'actor', id: '__proto__' }, { x: 300_000, y: 400_000 }, editorActors(model.draft, undefined, {}));
    expect(Object.hasOwn(next.positions, '__proto__')).toBe(true);
    expect(editorActors(next.draft, undefined, next.positions)['__proto__']).toEqual({ x: 300_000, y: 400_000 });
    expect(Object.getPrototypeOf(next.positions)).toBe(Object.prototype);
  });
  it('keeps a complete independent gesture snapshot so cancellation can restore every affected seat', () => {
    const original = fixture(), rollback = cloneEditorModel(original);
    const moved = moveEditorTarget(original, { kind: 'anchor', id: 'rest' }, { x: 750_000, y: 700_000 }, actors);
    expect(moved.draft.anchors).not.toEqual(rollback.draft.anchors);
    expect(rollback).toEqual(original);
    moved.draft.anchors[0].slots[0].position.x = '10';
    expect(rollback.draft.anchors[0].slots[0].position.x).toBe('60');
  });
});
