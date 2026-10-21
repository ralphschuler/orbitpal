import {describe,it,expect} from 'vitest';
import {parseCareConfig,parseCarePreferences} from './life-care';
const config=()=>({version:1,decayPerMinute:[180,250,220,150],restoreUnits:[500,1500,500,1000],startBelow:[6500,6500,6500,6500],satisfiedAt:[8500,8500,8500,8500],weights:[100,100,100,100],priorities:[100,100,100,100],travelCostWeight:10,minimumDurationMs:4000,cooldownMs:4000,backoffMs:15000,hysteresis:1000000});
describe('care preference boundary',()=>{
 it('requires exact bot and copies only checked preferences',()=>{const value=parseCarePreferences({version:1,botId:'nova',revision:2,config:{...config(),grant:true},active:true},'nova');expect(value.config).toEqual(config());expect(value).not.toHaveProperty('active');expect(()=>parseCarePreferences({...value,botId:'other'},'nova')).toThrow();});
 it('checks every dimension and threshold ordering',()=>{for(const patch of [{decayPerMinute:[0,0,0]},{restoreUnits:[5001,1,1,1]},{weights:[0,1,1,1]},{priorities:[1001,1,1,1]},{startBelow:[8500,1,1,1]},{satisfiedAt:[6500,8500,8500,8500]},{hysteresis:10000000001},{minimumDurationMs:0},{cooldownMs:600001},{backoffMs:.1}])expect(()=>parseCareConfig({...config(),...patch})).toThrow();});
});
