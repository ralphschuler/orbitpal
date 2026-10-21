/** UI mirror of the native start_scene participant bound. WorldConfig still
 * supports 64 saved actors; this helper never edits or truncates a layout. */
export const LIFE_SCENE_PARTICIPANT_LIMIT = 8;
export const LIFE_SCENE_LIMIT_HINT = "Life kann bis zu {limit} Figuren gleichzeitig starten.";
export const LIFE_SCENE_OVER_LIMIT = "Diese Anordnung enthält {count} Figuren. Life unterstützt höchstens {limit} gleichzeitig. Entferne Figuren aus der Anordnung, um Life zu starten. Deine gespeicherte Anordnung wird nicht automatisch gekürzt.";
export function lifeSceneParticipants(members:readonly string[]) {
  const count=members.length,limit=LIFE_SCENE_PARTICIPANT_LIMIT;
  return {count,limit,canAdd:count<limit,canStart:count>0&&count<=limit,overLimit:count>limit};
}
