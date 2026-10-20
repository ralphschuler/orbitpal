import { describe, expect, it, vi } from 'vitest';
import { updateCredentialSession, type CredentialSessionSnapshot } from './credential-session';
import { catalogues, translate } from './i18n';
import { readFileSync } from 'node:fs';

const status=(enabled:boolean,locked=false):CredentialSessionSnapshot=>({supported:true,enabled,locked,rememberedKeys:0});
describe('Operator credential-session changes',()=>{
  it('uses only the requested operator command on success',async()=>{
    const request=vi.fn().mockResolvedValue(status(true));
    expect(await updateCredentialSession(request,'set_credential_session',{enabled:true},String)).toEqual({status:status(true),error:''});
    expect(request.mock.calls).toEqual([['set_credential_session',{enabled:true}]]);
  });
  it('rechecks a failed disable instead of showing a stale enabled value or retrying',async()=>{
    const request=vi.fn().mockRejectedValueOnce('synthetic-write-error').mockResolvedValueOnce(status(false));
    expect(await updateCredentialSession(request,'set_credential_session',{enabled:false},error=>'shown: '+error)).toEqual({status:status(false),error:'shown: synthetic-write-error'});
    expect(request.mock.calls).toEqual([['set_credential_session',{enabled:false}],['get_credential_session']]);
  });
  it('does not restore or enable access when Lock or its status recheck fails',async()=>{
    const request=vi.fn().mockRejectedValue('synthetic-read-error');
    const result=await updateCredentialSession(request,'lock_credential_session',{},String);
    expect(result.status).toBeUndefined();expect(result.error).toBe('synthetic-read-error');
    expect(request.mock.calls).toEqual([['lock_credential_session',{}],['get_credential_session']]);
  });
  it('localizes a native locked-session failure while retaining its locked status',async()=>{
    const message='Die Schlüssel sind für diese Sitzung gesperrt. Gib sie unter Modelle erneut frei.';
    for(const language of ['en','fr','es'] as const){
      const request=vi.fn().mockRejectedValueOnce(message).mockResolvedValueOnce(status(true,true));
      const result=await updateCredentialSession(request,'set_credential_session',{enabled:true},error=>translate(String(error),{},language));
      expect(result.status?.locked).toBe(true);expect(result.error).not.toBe(message);expect(result.error).not.toContain('gesperrt');
    }
  });
});


it('localizes each bounded native session diagnostic in every supported UI language',()=>{
  const files=['credential_session.rs','credential_session_settings.rs'];
  const diagnostics=files.flatMap(file=>{
    const source=readFileSync(new URL('../../src-tauri/src/'+file,import.meta.url),'utf8').split('#[cfg(test)]')[0];
    return [...source.matchAll(/(?:const [A-Z_]+: &str = |Err\()"((?:[^"\\]|\\.)*)"/g)].map(match=>JSON.parse('"'+match[1]+'"'));
  });
  expect(diagnostics.length).toBeGreaterThanOrEqual(7);
  for(const message of diagnostics)for(const language of ['en','fr','es'] as const){
    expect(catalogues.some(catalogue=>Object.hasOwn(catalogue[language],message)),language+': '+message).toBe(true);
    expect(translate(message,{},language)).not.toBe(message);
  }
});
