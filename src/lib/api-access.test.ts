import { describe, expect, it } from 'vitest';
import { ApiBindingReads, apiAccessError, createApiConnection, getApiConnections, mutateApiConnection, parseApiBinding, parseApiSnapshot, safeApiDiagnostics, type ApiRequest } from './api-access';

const scope={service:'openai',destination:'https://api.openai.com/v1',authStyle:'bearer'};
const account=(patch:Record<string,unknown>={})=>({...scope,id:'account-1',label:'Main account',keyStatus:'registered',source:'operator',writePolicy:'replaceable',sharedConnectionCount:1,externalReferences:false,...patch});
const snapshot=(patch:Record<string,unknown>={})=>({version:1,availability:'ready',reason:null,revision:2,connections:[account()],defaults:[{...scope,id:'default-1',connectionId:'account-1'}],legacyUnboundCount:0,...patch});
const binding=(patch:Record<string,unknown>={})=>({binding:scope,unused:false,selectedRef:null,defaultRef:'default-1',defaultConnectionId:'account-1',compatibleIds:['account-1'],status:'unbound',reason:null,...patch});
function deferred<T>(){let resolve!:(value:T)=>void;const promise=new Promise<T>(next=>resolve=next);return {promise,resolve};}

describe('global API access metadata contract',()=>{
  it('returns copied frozen metadata and distinguishes registered from remotely checked',()=>{
    const source=snapshot(),parsed=parseApiSnapshot(source);source.connections[0].label='changed';
    expect(parsed.connections[0].label).toBe('Main account');expect(Object.isFrozen(parsed.connections[0])).toBe(true);
    expect(Object.hasOwn(parsed.connections[0],'connected')).toBe(false);expect(Object.hasOwn(parsed.connections[0],'value')).toBe(false);
  });
  it('accepts legacy unbound counts beyond connection capacity without truncation',()=>{
    expect(parseApiSnapshot(snapshot({legacyUnboundCount:5120})).legacyUnboundCount).toBe(5120);
    expect(()=>parseApiSnapshot(snapshot({legacyUnboundCount:Number.MAX_SAFE_INTEGER+1}))).toThrow();
  });
  it('keeps unavailable revision unknown instead of fabricating a zero CAS baseline',()=>{
    expect(parseApiSnapshot(snapshot({availability:'unavailable',reason:'unsupported_schema',revision:null,connections:[],defaults:[]})).revision).toBeNull();
    expect(()=>parseApiSnapshot(snapshot({availability:'unavailable',reason:'database',revision:0,connections:[],defaults:[]}))).toThrow();
  });
  it.each([
    snapshot({version:2}),snapshot({revision:NaN}),snapshot({revision:Number.MAX_SAFE_INTEGER+1}),
    snapshot({connections:[account({id:'bad/id'})]}),snapshot({connections:[account({label:'é'.repeat(61)})]}),
    snapshot({connections:[account({destination:'https://user:secret@example.com/v1'})]}),
    snapshot({connections:[account({destination:'https://api.openai.com/v1?token=hidden'})]}),
    snapshot({connections:[account({keyStatus:'connected'})]}),snapshot({connections:[account({writePolicy:'unrestricted'})]}),
    snapshot({connections:[account({sharedConnectionCount:0})]}),snapshot({connections:[account({externalReferences:'false'})]}),
    snapshot({connections:[account(),account()]}),snapshot({defaults:[{...scope,id:'account-1',connectionId:'account-1'}]}),
    snapshot({defaults:[{...scope,destination:'https://other.example/v1',id:'default-1',connectionId:'account-1'}]}),
    snapshot({value:'synthetic-secret-never-display'}),snapshot({connections:[account({apiKey:'synthetic-secret-never-display'})]}),
  ])('rejects malformed, mixed-scope or secret-bearing native metadata',value=>expect(()=>parseApiSnapshot(value)).toThrow());
  it('does not execute metadata getters',()=>{
    let invoked=false;const value=snapshot();Object.defineProperty(value,'revision',{get(){invoked=true;return 1;}});
    expect(()=>parseApiSnapshot(value)).toThrow();expect(invoked).toBe(false);
  });
  it('preserves blank, named/default and invalid choices without filling a blank from the available default',()=>{
    const empty=parseApiBinding(binding());expect(empty.selectedRef).toBeNull();expect(empty.status).toBe('unbound');expect(empty.defaultRef).toBe('default-1');
    expect(parseApiBinding(binding({selectedRef:'default-1',status:'default'})).selectedRef).toBe('default-1');
    const invalid=parseApiBinding(binding({selectedRef:'removed-account',status:'invalid',reason:'scope_mismatch'}));
    expect(invalid.selectedRef).toBe('removed-account');expect(invalid.reason).toBe('scope_mismatch');
  });
  it('retains the read-only legacy sharing facts without calling them per-bot usage counts',()=>{
    const parsed=parseApiSnapshot(snapshot({connections:[account({source:'legacy',writePolicy:'shared_legacy_read_only',sharedConnectionCount:2,externalReferences:true})]}));
    expect(parsed.connections[0].writePolicy).toBe('shared_legacy_read_only');expect(parsed.connections[0].externalReferences).toBe(true);
    expect(Object.hasOwn(parsed.connections[0],'botCount')).toBe(false);
  });
  it('reads only the metadata command and rejects arbitrary returned secret/error content',async()=>{
    const calls:string[]=[];const request:ApiRequest=async command=>{calls.push(command);return snapshot();};
    await getApiConnections(request);expect(calls).toEqual(['get_api_connections']);
    expect(apiAccessError(new Error('synthetic-private-write-value'))).not.toContain('synthetic-private-write-value');
  });
  it('preserves only exact fixed native diagnostics, including actionable stale-CAS errors',()=>{
    for(const text of safeApiDiagnostics){expect(apiAccessError(text)).toBe(text);expect(apiAccessError(new Error(text))).toBe(text);expect(apiAccessError(text+' synthetic-private-value')).not.toContain('synthetic-private-value');}
    expect(apiAccessError('Die API-Zugänge wurden inzwischen geändert. Bitte neu laden.')).toBe('Die API-Zugänge wurden inzwischen geändert. Bitte neu laden.');
  });
  it('exposes a real created entry before a separately failed key write',async()=>{
    const calls:{command:string;args?:Record<string,unknown>}[]=[];
    const request:ApiRequest=async(command,args)=>{calls.push({command,args});if(command==='create_api_connection')return {snapshot:snapshot({revision:3,connections:[account({keyStatus:'missing'})]}),connectionId:'account-1'};throw new Error('synthetic denied key write');};
    const created=await createApiConnection(request,2,{...scope,service:'openai',authStyle:'bearer',label:'Main account'});
    expect(created.snapshot.connections[0].keyStatus).toBe('missing');
    await expect(mutateApiConnection(request,'set_api_connection_secret',{expectedRevision:created.snapshot.revision,id:created.connectionId,value:'synthetic-only'})).rejects.toThrow();
    expect(calls.map(item=>item.command)).toEqual(['create_api_connection','set_api_connection_secret']);
    expect(calls[1].args?.expectedRevision).toBe(3);expect(JSON.stringify(created)).not.toContain('synthetic-only');
  });
  it('ignores an old endpoint or old bot query that resolves after a replacement',async()=>{
    const old=deferred<unknown>(),next=deferred<unknown>();let call=0;const request:ApiRequest=async()=>++call===1?old.promise:next.promise;
    const reads=new ApiBindingReads();const first=reads.load(request,'dialog',{baseUrl:'https://old.example'},()=>true);
    const second=reads.load(request,'dialog',{baseUrl:'https://new.example'},()=>true);
    next.resolve(binding({binding:{...scope,destination:'https://new.example/v1'}}));expect((await second)?.binding?.destination).toBe('https://new.example/v1');
    old.resolve(binding());expect(await first).toBeNull();
  });
  it('invalidates an outstanding selector read on unmount or bot change',async()=>{
    const held=deferred<unknown>();const reads=new ApiBindingReads();const result=reads.load(async()=>held.promise,'speech',{},()=>true);reads.invalidate();held.resolve(binding());expect(await result).toBeNull();
  });
});
