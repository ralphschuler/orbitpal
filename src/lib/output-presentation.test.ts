import {describe,it,expect,vi} from 'vitest';
import {OutputPreviewQueue,PresentationVersions,validScale} from './output-presentation';
describe('Field-scoped output preview',()=>{
  it('coalesces sliders without submitting other draft fields and flushes across bots',async()=>{
    const calls:any[]=[];let release!:()=>void;const pending=new Promise<void>(resolve=>release=resolve);
    const queue=new OutputPreviewQueue(async value=>{calls.push(value);if(calls.length===1)await pending;},vi.fn());
    queue.put({botId:'nova',field:'scale',value:1});await Promise.resolve();queue.put({botId:'nova',field:'scale',value:1.2});queue.put({botId:'nova',field:'scale',value:1.7});queue.put({botId:'other',field:'scale',value:.5});release();await queue.flush();
    expect(calls).toEqual([{botId:'nova',field:'scale',value:1},{botId:'nova',field:'scale',value:1.7},{botId:'other',field:'scale',value:.5}]);
  });
  it('rejects nonfinite/out of range scales and continues safely after an output error',async()=>{
    for(const value of [NaN,Infinity,-Infinity,0,.299,2.501,'1'])expect(validScale(value)).toBe(false);
    const failure=vi.fn();const send=vi.fn().mockRejectedValueOnce(new Error('fixture')).mockResolvedValue({});const queue=new OutputPreviewQueue(send,failure);
    queue.put({botId:'nova',field:'scale',value:NaN});queue.put({botId:'nova',field:'scale',value:1});queue.put({botId:'other',field:'scale',value:2});await queue.flush();expect(send).toHaveBeenCalledTimes(2);expect(failure).toHaveBeenCalledOnce();
  });
  it('keeps new preview scale/layout when an initial or profile response arrives late',()=>{
    const versions=new PresentationVersions();const snapshot={botId:'nova',revision:2,scale:1.7,desktop:{active:true,referencePhysicalHeight:1440}};
    expect(versions.accept(snapshot,'nova')).toBe(true);expect(versions.accept({...snapshot,revision:1,scale:1},'nova')).toBe(false);expect(versions.accept({...snapshot,botId:'other'},'nova')).toBe(false);
    expect(versions.accept({...snapshot,desktop:{active:true,referencePhysicalHeight:0}},'nova')).toBe(false);
  });
});
