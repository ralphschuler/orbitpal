export interface CredentialSessionSnapshot {
  supported: boolean;
  enabled: boolean;
  locked: boolean;
  rememberedKeys: number;
}
export type CredentialSessionRequest = (command:string, args?:Record<string, unknown>) => Promise<CredentialSessionSnapshot>;

// A failed disable can still have cleared the live session. Always read back the
// native state after a failed mutation, never retry the mutation automatically.
export async function updateCredentialSession(
  request:CredentialSessionRequest,
  command:'set_credential_session'|'lock_credential_session',
  args:Record<string, unknown>,
  errorText:(error:unknown)=>string,
):Promise<{status?:CredentialSessionSnapshot;error:string}> {
  try { return { status:await request(command,args), error:'' }; }
  catch(error) {
    const status = await request('get_credential_session').catch(()=>undefined);
    return {status,error:errorText(error)};
  }
}
