import { describe,it,expect,vi } from 'vitest';
import { LifeUI } from './life-ui';
import { actionReasons,actionStatusLabels,actionFailureLabels,cancellationLabels,parseLifeActions,requestQuietRest,type LifeActionsSnapshot } from './life-actions';
import catalogue from './locales/life-actions.json';
const connection='22222222-2222-2222-2222-222222222222';
const action={actorId:'nova',worldId:'world_nova',ordinal:1};
const snapshot=():LifeActionsSnapshot=>({version:1,instanceId:'11111111-1111-1111-1111-111111111111',sequence:3,botId:'nova',availability:'ready',reason:null,visualAvailable:false,confirmed:{actionId:action,activity:'rest',phase:'started',cancellation:null,revision:2},runtime:{actionId:action,presentation:'native_rest_dwell',status:'started',failure:null,activeMilliseconds:1234}});
const held=<T>()=>{let resolve!:(value:T)=>void;const promise=new Promise<T>(done=>resolve=done);return{promise,resolve};};
const flush=async()=>{for(let i=0;i<12;i++)await Promise.resolve();};
describe('Confirmed local action boundary',()=>{
  it('projects actual runtime and durable facts separately and strips private authority',()=>{
    const raw={...snapshot(),sessionId:'private',credentials:'private',runtime:{...snapshot().runtime!,output:'private',lease:'private'},confirmed:{...snapshot().confirmed!,phase:'completed',revision:4}};
    const result=parseLifeActions(raw,'nova');expect(result.confirmed?.phase).toBe('completed');expect(result.runtime?.status).toBe('started');expect(result.runtime?.activeMilliseconds).toBe(1234);expect(JSON.stringify(result)).not.toContain('private');
    raw.confirmed.actionId={...action,ordinal:2};expect(parseLifeActions(raw,'nova').confirmed?.actionId.ordinal).toBe(2);
  });
  it('preserves known facts on unavailability without inventing current readiness',()=>{
    const raw={...snapshot(),availability:'unavailable',reason:'system_unavailable'};expect(parseLifeActions(raw,'nova').confirmed).not.toBeNull();
    expect(parseLifeActions({...raw,availability:'not_initialized',reason:'life_not_initialized',confirmed:null,runtime:null},'nova').runtime).toBeNull();
  });
  it('rejects unsupported schemas, wrong owner/identity, unsafe numbers and inconsistent status claims',()=>{
    for(const change of [{version:2},{botId:'other'},{instanceId:'not-a-native-instance'},{sequence:NaN},{sequence:Infinity},{sequence:Number.MAX_SAFE_INTEGER+1},{visualAvailable:true},{availability:'ready',reason:'busy'},{availability:'not_initialized',reason:null},{availability:'unavailable',reason:'life_not_initialized'},{confirmed:{...snapshot().confirmed,actionId:{...action,actorId:'other'}}},{confirmed:{...snapshot().confirmed,phase:'cancelled',cancellation:null}},{confirmed:{...snapshot().confirmed,cancellation:'operator'}},{runtime:{...snapshot().runtime,failure:'timeout'}},{runtime:{...snapshot().runtime,status:'failed',failure:null}},{runtime:{...snapshot().runtime,activeMilliseconds:-1}},{runtime:{...snapshot().runtime,actionId:{...action,ordinal:0}}}])expect(()=>parseLifeActions({...snapshot(),...change},'nova')).toThrow();
  });
  it('uses only an acknowledged Studio proof for one explicit rest request',async()=>{
    const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?{connectionId:connection}:command==='start_life_rest'?snapshot():true);const ui=new LifeUI(invoke,vi.fn());await ui.connect();const onProof=vi.fn();
    expect(await requestQuietRest(invoke,ui,'nova',()=>true,onProof)).toEqual(snapshot());expect(onProof).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith('start_life_rest',{botId:'nova',ui:{connectionId:connection,sequence:2}});
    expect(invoke.mock.calls.map(call=>call[0])).toEqual(['connect_life_ui','report_life_ui','report_life_ui','start_life_rest']);ui.dispose();
  });
  it('does not clear or dispatch when real blockers exist, including changes before acknowledgement',async()=>{
    const pending=held<unknown>();let reports=0,current=true;const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?{connectionId:connection}:command==='report_life_ui'&&++reports===2?pending.promise:true);const ui=new LifeUI(invoke,vi.fn());await ui.connect();
    await expect(requestQuietRest(invoke,ui,'nova',()=>false)).rejects.toThrow('nicht mehr aktuell');expect(reports).toBe(1);
    const request=requestQuietRest(invoke,ui,'nova',()=>current);const rejected=expect(request).rejects.toThrow('nicht mehr aktuell');await flush();current=false;pending.resolve(true);await rejected;expect(invoke.mock.calls.some(call=>call[0]==='start_life_rest')).toBe(false);ui.dispose();
  });
  it('ignores a late result after Stop/bot replacement and never starts a successor',async()=>{
    const pending=held<unknown>();let current=true;const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?{connectionId:connection}:command==='start_life_rest'?pending.promise:true);const ui=new LifeUI(invoke,vi.fn());await ui.connect();const result=requestQuietRest(invoke,ui,'nova',()=>current);await flush();current=false;pending.resolve(snapshot());await expect(result).resolves.toBeUndefined();expect(invoke.mock.calls.filter(call=>call[0]==='start_life_rest')).toHaveLength(1);ui.dispose();
  });
  it('requires a matching actual rest acknowledgement and never substitutes a mode start',async()=>{
    const wrong={...snapshot(),runtime:{...snapshot().runtime!,actionId:{...action,ordinal:2}}};const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?{connectionId:connection}:command==='start_life_rest'?wrong:true);const ui=new LifeUI(invoke,vi.fn());await ui.connect();await expect(requestQuietRest(invoke,ui,'nova',()=>true)).rejects.toThrow('ungültige Daten');expect(invoke.mock.calls.some(call=>['set_life_mode','send_message','synthesize_speech'].includes(call[0]))).toBe(false);ui.dispose();
  });
  it('covers all fixed status vocabulary and keeps placeholders in all interface translations',()=>{
    const fixed=[...Object.values(actionReasons),...Object.values(actionStatusLabels),...Object.values(actionFailureLabels),...Object.values(cancellationLabels),'Lokale Lebensaktionen werden ausschließlich im Studio gesteuert.','Die stille Pause ist momentan nicht verfügbar. Prüfe Life, Studio, Bühne und Ausgabe.','Bitte die lokale Lebensaktion vor der automatischen Antwort beenden.','Visuelle Lebensaktionen benötigen eine bestätigte Darstellung. Diese Verbindung folgt in einer späteren Version.'];
    for(const language of ['en','fr','es'] as const){for(const key of fixed)expect(catalogue[language][key as keyof typeof catalogue.en],`${language}: ${key}`).toBeTruthy();for(const[source,translated]of Object.entries(catalogue[language]))expect([...translated.matchAll(/\{\w+\}/g)].map(match=>match[0]).sort()).toEqual([...source.matchAll(/\{\w+\}/g)].map(match=>match[0]).sort());}
  });
});
