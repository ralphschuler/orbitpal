import { describe, expect, it } from 'vitest';
import { parseMotionBinding, parseWorldMotionCommand, WorldMotionFollower,
  type MotionBinding, type MotionOffer } from './world-motion';

// Pure protocol/kinematic fixtures. These do not establish native authority,
// actual visible frames, a completed station action or a Life need effect.
function binding(): MotionBinding {
  return { actorId:'nova',sourceWorldId:'life-nova',renderSessionId:'render-1',bindingSequence:1,
    worldInstanceId:'world-1',stageId:'stage-1',configRevision:1,topologyRevision:1,layoutRevision:1,
    assetRevision:1,measurementRevision:1,referenceRevision:1,viewport:{width:1000,height:800},
    safeCenters:{left:20,top:40,right:980,bottom:760},maximumFootprint:{width:40,height:80},
    envelopeExtents:{left:20,top:40,right:20,bottom:40},referenceLocal:{x:0,y:1,z:0},
    position:{x:500,y:400},groundBasis:{right:{x:.01,z:0},down:{x:0,z:.02}},
    modelForwardYaw:0,profileScale:1,effectiveScale:1,scale:1,reducedMotion:false,
    walkSupported:true,stationSupported:true };
}
function station(value = binding()): MotionOffer {
  return { version:1,type:'offer',sequence:1,activity:'rest',minimumActiveMs:1000,
    from:value.position,target:value.position,maximumSpeed:100,acceleration:500,validForMs:5000,
    fence:{actorId:value.actorId,renderSessionId:value.renderSessionId,bindingSequence:1,
      worldInstanceId:value.worldInstanceId,stageId:value.stageId,configRevision:1,topologyRevision:1,
      layoutRevision:1,assetRevision:1,measurementRevision:1,referenceRevision:1,
      actionId:{actorId:value.actorId,worldId:value.sourceWorldId,ordinal:1},deliveryId:'delivery-1'} };
}
describe('trusted station offer contract', () => {
  it('accepts only implemented closed activities and positive bounded dwell below the offer deadline', () => {
    for (const activity of ['rest','recharge','snack','play','socialize'] as const)
      expect(parseWorldMotionCommand({...station(),activity})).not.toBeNull();
    for (const activity of ['look_at','wave','other'])
      expect(parseWorldMotionCommand({...station(),activity})).toBeNull();
    for (const minimumActiveMs of [0,-1,.5,5000,60001,NaN,Infinity])
      expect(parseWorldMotionCommand({...station(),minimumActiveMs})).toBeNull();
    expect(parseWorldMotionCommand({...station(),minimumActiveMs:60_000,validForMs:75_000})).not.toBeNull();
    expect(parseWorldMotionCommand({...station(),minimumActiveMs:60_000,validForMs:75_001})).toBeNull();
    expect(parseWorldMotionCommand({...station(),activity:'move_to',minimumActiveMs:0})).not.toBeNull();
    expect(parseWorldMotionCommand({...station(),activity:'move_to'})).toBeNull();
  });
  it('cannot run a station from a legacy or explicitly unsupported measured binding', () => {
    const { stationSupported: _stationSupported, ...legacy } = binding();
    for (const value of [legacy,{...binding(),stationSupported:false}]) {
      const parsed=parseMotionBinding(value)!; expect(parsed.stationSupported).toBe(false);
      const follower=new WorldMotionFollower();expect(follower.bind(parsed,0)).toBe(true);
      expect(follower.receive(station(parsed),0)).toBe('rejected');
      expect(follower.snapshot().reason).toBe('unsupported_activity');
      expect(follower.sample(16,.016)).toBeNull();
    }
  });
  it('rejects accessor capabilities and dwell fields without invoking their getters', () => {
    let reads=0; const value={...binding()},offer={...station()};
    Object.defineProperty(value,'stationSupported',{enumerable:true,get(){reads++;return true;}});
    Object.defineProperty(offer,'minimumActiveMs',{enumerable:true,get(){reads++;return 1000;}});
    expect(parseMotionBinding(value)).toBeNull();expect(parseWorldMotionCommand(offer)).toBeNull();
    expect(reads).toBe(0);
  });
  it('treats changed activity or dwell on the same action identity as a conflicting offer', () => {
    for (const change of [{activity:'play' as const},{minimumActiveMs:1001}]) {
      const follower=new WorldMotionFollower(),value=binding(),offer=station(value);
      follower.bind(value,0);expect(follower.receive(offer,0)).toBe('accepted');
      expect(follower.receive({...offer,...change,sequence:2},1)).toBe('rejected');
      expect(follower.snapshot().reason).toBe('conflicting_offer');
      expect(follower.sample(16,.016)).toBeNull();
    }
  });
});
