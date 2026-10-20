import {describe,it,expect} from 'vitest';
import {pointerTarget,stepTarget,targetPoint} from './world-target';
const bounds={left:100,top:50,right:900,bottom:450,width:1000,height:500};
describe('visual movement targets',()=>{
 it('maps resized CSS coordinates into native safe centers',()=>{expect(pointerTarget(310,145,{left:10,top:20,width:600,height:250},bounds)).toEqual({x:500000,y:500000});});
 it('keeps an entire measured character inside its movement area',()=>{expect(pointerTarget(-10,1000,{left:0,top:0,width:500,height:250},bounds)).toEqual({x:100000,y:900000});});
 it('uses the same clamped targets for arrows and coarse arrows',()=>{expect(stepTarget({x:895000,y:100000},'ArrowRight',true,bounds)).toEqual({x:900000,y:100000});expect(stepTarget({x:500000,y:500000},'ArrowUp',false,bounds)).toEqual({x:500000,y:490000});expect(stepTarget({x:0,y:0},'Enter',false,bounds)).toBeUndefined();});
 it('quantizes fractional edges inward so the host accepts boundary targets',()=>{const fractional={...bounds,left:100.0004,right:899.9996,top:50.0004,bottom:449.9996};const a=targetPoint(-100,-100,fractional),b=targetPoint(10000,10000,fractional);expect(a.x/1_000_000*bounds.width).toBeGreaterThanOrEqual(fractional.left);expect(a.y/1_000_000*bounds.height).toBeGreaterThanOrEqual(fractional.top);expect(b.x/1_000_000*bounds.width).toBeLessThanOrEqual(fractional.right);expect(b.y/1_000_000*bounds.height).toBeLessThanOrEqual(fractional.bottom);});
 it('rejects unmeasured or invalid geometry rather than inventing a target',()=>{expect(()=>pointerTarget(0,0,{left:0,top:0,width:0,height:250},bounds)).toThrow();expect(()=>targetPoint(NaN,0,bounds)).toThrow();expect(()=>targetPoint(0,0,{...bounds,left:950})).toThrow();});
});
