import { afterEach, describe, expect, it, vi } from 'vitest';
import { LifeUI } from './life-ui';
const first = '11111111-1111-1111-1111-111111111111';
const second = '22222222-2222-2222-2222-222222222222';
const held = <T>() => { let resolve!: (value:T)=>void; const promise=new Promise<T>(done=>resolve=done); return { promise,resolve }; };
const flush = async()=>{for(let i=0;i<12;i++)await Promise.resolve();};
afterEach(()=>vi.useRealTimers());

describe('Session-only Studio native blocker acknowledgements', () => {
  it('starts blocked with a native nonce and serializes newer blockers after an older clear', async () => {
    const clear=held<unknown>(); const changed=vi.fn(); let reports=0;
    const invoke=vi.fn(async(command:string,args?:Record<string,unknown>)=>command==='connect_life_ui'?{connectionId:first}:command==='report_life_ui'&&++reports===2?clear.promise:true);
    const ui=new LifeUI(invoke,changed); await ui.connect();
    const old=ui.report(false,false), newer=ui.report(true,true);await flush();
    const payloads=()=>invoke.mock.calls.filter(call=>call[0]==='report_life_ui').map(call=>call[1]);
    expect(payloads()).toEqual([{report:{connectionId:first,sequence:1,blocked:true,activity:false}},{report:{connectionId:first,sequence:2,blocked:false,activity:false}}]);
    clear.resolve(true);await old;await newer;
    expect(payloads()[2]).toEqual({report:{connectionId:first,sequence:3,blocked:true,activity:true}});
    expect(invoke.mock.calls.every(call=>!['set_life_mode','send_autopilot','send_message'].includes(call[0]))).toBe(true);ui.dispose();
  });
  it('fails closed on a false acknowledgement and never reports a fabricated clear', async () => {
    let accepted=true;const changed=vi.fn();const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?{connectionId:first}:accepted);
    const ui=new LifeUI(invoke,changed);await ui.connect();accepted=false;
    await expect(ui.report(true,true)).rejects.toThrow('nicht bestätigt');
    await expect(ui.report(false,false)).rejects.toThrow();
    expect(invoke).toHaveBeenCalledWith('disconnect_life_ui',{connectionId:first});expect(changed).toHaveBeenLastCalledWith(false);ui.dispose();
  });
  it('disconnects only a late old identity after a timed-out connect and allows explicit reconnect', async () => {
    vi.useFakeTimers();const late=held<unknown>();let connections=0;
    const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?(++connections===1?late.promise:{connectionId:second}):true);
    const ui=new LifeUI(invoke,vi.fn(),100);const old=ui.connect();const rejected=expect(old).rejects.toThrow();await vi.advanceTimersByTimeAsync(100);await rejected;
    await ui.connect();late.resolve({connectionId:first});await flush();
    expect(invoke).toHaveBeenCalledWith('disconnect_life_ui',{connectionId:first});
    expect(invoke).not.toHaveBeenCalledWith('disconnect_life_ui',{connectionId:second});
    await ui.report(false,false);ui.dispose();expect(invoke).toHaveBeenCalledWith('disconnect_life_ui',{connectionId:second});
  });
  it('does not let an old in-flight acknowledgement revive a disposed or replaced reporter', async () => {
    const pending=held<unknown>();let reports=0;const changed=vi.fn();
    const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?{connectionId:first}:command==='report_life_ui'&&++reports===2?pending.promise:true);
    const ui=new LifeUI(invoke,changed);await ui.connect();const work=ui.report(false,false);const rejected=expect(work).rejects.toThrow();await flush();ui.dispose();pending.resolve(true);await rejected;
    expect(changed).toHaveBeenLastCalledWith(false);await expect(ui.report(false,false)).rejects.toThrow();
  });
  it('times out missing report acknowledgements and disconnects the affected identity', async () => {
    vi.useFakeTimers();let reports=0;const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?{connectionId:first}:command==='report_life_ui'&&++reports===2?new Promise(()=>{}):true);
    const ui=new LifeUI(invoke,vi.fn(),100);await ui.connect();const work=ui.report(true,true);const rejected=expect(work).rejects.toThrow();await vi.advanceTimersByTimeAsync(100);await rejected;
    expect(invoke).toHaveBeenCalledWith('disconnect_life_ui',{connectionId:first});ui.dispose();
  });
  it('rejects malformed connection identities before sending any status', async () => {
    const invoke=vi.fn(async()=>({connectionId:'untrusted'}));const ui=new LifeUI(invoke,vi.fn());await expect(ui.connect()).rejects.toThrow();expect(invoke).toHaveBeenCalledTimes(1);ui.dispose();
  });
  it('returns only an actual acknowledged proof and dispatches before queued heartbeats without holding blockers behind the command', async () => {
    const slow=held<unknown>(),clear=held<unknown>();const order:string[]=[];let reports=0;
    const invoke=vi.fn(async(command:string,args?:Record<string,unknown>)=>{
      if(command==='connect_life_ui')return{connectionId:first};
      if(command==='report_life_ui'){const report=args?.report as {sequence:number;blocked:boolean};order.push(`report:${report.sequence}:${report.blocked}`);return++reports===2?clear.promise:true;}
      if(command==='start_life_rest'){order.push('start');return slow.promise;}
      return true;
    });
    const ui=new LifeUI(invoke,vi.fn());await ui.connect();
    const rest=ui.reportProof(false,true,proof=>{expect(proof).toEqual({connectionId:first,sequence:2});return invoke('start_life_rest',{botId:'nova',ui:proof});});
    const blocker=ui.report(true,true);await flush();clear.resolve(true);await blocker;
    expect(order).toEqual(['report:1:true','report:2:false','start','report:3:true']);
    slow.resolve('confirmed');await expect(rest).resolves.toBe('confirmed');
    await expect(ui.report(false,false)).resolves.toEqual({connectionId:first,sequence:4});ui.dispose();
  });
  it('never dispatches a proof callback after rejection or reporter disposal', async () => {
    const clear=held<unknown>();let reports=0;const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?{connectionId:first}:command==='report_life_ui'&&++reports===2?clear.promise:true);
    const ui=new LifeUI(invoke,vi.fn());await ui.connect();const dispatch=vi.fn(async()=>true);
    const result=ui.reportProof(false,true,dispatch);const rejected=expect(result).rejects.toThrow();await flush();ui.dispose();clear.resolve(true);await rejected;expect(dispatch).not.toHaveBeenCalled();
  });
  it('keeps transport acknowledgement separate from a rejected operator command', async () => {
    const invoke=vi.fn(async(command:string)=>command==='connect_life_ui'?{connectionId:first}:true);const ui=new LifeUI(invoke,vi.fn());await ui.connect();
    await expect(ui.reportProof(false,true,async()=>{throw new Error('operator denied');})).rejects.toThrow('operator denied');
    await expect(ui.report(true,true)).resolves.toEqual({connectionId:first,sequence:3});
    expect(invoke.mock.calls.filter(call=>call[0]==='disconnect_life_ui')).toHaveLength(0);ui.dispose();
  });
});
