import type {Development} from './social-experience';

/** Snapshot data may refresh while a separately edited settings form is dirty. */
export function refreshSnapshotDraft<T>(draft:T|undefined, previous:T|undefined, incoming:T, reset=false):T {
  if (!reset && draft !== undefined && previous !== undefined
      && JSON.stringify(draft) !== JSON.stringify(previous)) return draft;
  return structuredClone(incoming);
}

type SocialPreferences=Pick<Development,'traits'|'interests'|'routines'>;
const socialPreferences=(value:Development):SocialPreferences=>({
  traits:value.traits,interests:value.interests,routines:value.routines,
});

/** Earned progress is native data, independent of the operator's preference edits. */
export function refreshDevelopmentDraft(draft:Development|undefined, previous:Development|undefined, incoming:Development, reset=false):Development {
  const preferences=refreshSnapshotDraft(draft&&socialPreferences(draft),previous&&socialPreferences(previous),socialPreferences(incoming),reset);
  const next={...structuredClone(incoming),...preferences};
  // A failed explicit discard remains an unsaved intent, never a source of
  // goal identity, target or earned progress for a different/current goal.
  if (!reset && draft?.goal?.status==='discarded' && previous?.goal?.status!=='discarded'
      && next.goal?.id===draft.goal.id) next.goal.status='discarded';
  return next;
}
