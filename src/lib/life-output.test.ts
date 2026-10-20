import {describe,it,expect} from 'vitest';
import {LifeOutputVersions,parseLifeOutput} from './life-output';
const route={version:1,botId:'james',sequence:1,routeId:'route_a',output:'overlay',state:'opening',reason:null};
describe('Life desktop output projection',()=>{
 it('does not resurrect stopped output through late initial snapshots or events',()=>{const versions=new LifeOutputVersions();expect(versions.accept({...route,sequence:2,state:'visible'})).toBeDefined();expect(versions.accept({...route,sequence:3,state:'inactive',routeId:null,output:null})).toBeDefined();expect(versions.accept(route)).toBeUndefined();expect(versions.accept({...route,sequence:2,state:'visible'})).toBeUndefined();});
 it('isolates bots and rejects a mismatched overlay snapshot',()=>{const versions=new LifeOutputVersions();expect(versions.accept(route,'nova')).toBeUndefined();expect(versions.accept(route,'james')).toBeDefined();expect(versions.accept({...route,botId:'nova'})).toBeDefined();});
 it('requires explicit inactive tombstones and actionable unavailable state',()=>{expect(()=>parseLifeOutput({...route,state:'inactive'})).toThrow();expect(()=>parseLifeOutput({...route,state:'unavailable'})).toThrow();expect(parseLifeOutput({...route,state:'unavailable',reason:'window_unavailable'}).reason).toBe('window_unavailable');expect(()=>parseLifeOutput({...route,sequence:NaN})).toThrow();});
});
