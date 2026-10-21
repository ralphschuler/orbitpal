/** Compare the same CLI model value that persist() will save, without
 * mutating a draft or ignoring any other profile/document change. */
export function lifeStartContext(profile:any,documents:unknown):string {
  const model=profile?.model;
  const normalized=(model?.provider==='codex'||model?.provider==='claude-code')&&!model.model?.trim()
    ?{...profile,model:{...model,model:'default'}}:profile;
  return JSON.stringify([normalized,documents]);
}
