import { describe, expect, it } from 'vitest';
import { parseMotionBinding, parseWorldMotionCommand, WorldMotionFollower, WORLD_MAX_FRAME_SECONDS,
  type CssPoint, type FrameObservation, type MotionBinding, type MotionFence, type MotionOffer,
  type MotionProposal, type RenderEvidence } from './world-motion';

// These are synthetic after-render observations for a pure contract fixture.
// They do not execute WebGL, native output registration or actual arrival.
function binding(overrides: Partial<MotionBinding> = {}): MotionBinding {
  return { actorId: 'nova', sourceWorldId: 'nova-world', renderSessionId: 'render-1', bindingSequence: 1,
    worldInstanceId: 'world-observation-1', stageId: 'shared-stage', configRevision: 1, topologyRevision: 1,
    layoutRevision: 1, assetRevision: 1, measurementRevision: 1, referenceRevision: 1, viewport: { width: 1000, height: 800 },
    safeCenters: { left: 20, top: 40, right: 980, bottom: 760 }, maximumFootprint: { width: 40, height: 80 },
    position: { x: 500, y: 400 }, groundBasis: { right: { x: .01, z: 0 }, down: { x: 0, z: .02 } },
    referenceLocal: { x: 0, y: 1.45, z: 0 }, envelopeExtents: { left: 20, top: 40, right: 20, bottom: 40 },
    profileScale: 1, effectiveScale: 1, modelForwardYaw: 0, scale: 1, reducedMotion: false, walkSupported: true, stationSupported: false, ...overrides };
}
function fence(value: MotionBinding, ordinal = 1): MotionFence {
  return { actorId: value.actorId, renderSessionId: value.renderSessionId, bindingSequence: value.bindingSequence,
    worldInstanceId: value.worldInstanceId, stageId: value.stageId, configRevision: value.configRevision,
    topologyRevision: value.topologyRevision, layoutRevision: value.layoutRevision, assetRevision: value.assetRevision,
    measurementRevision: value.measurementRevision, referenceRevision: value.referenceRevision, actionId: { actorId: value.actorId, worldId: value.sourceWorldId, ordinal }, deliveryId: `delivery-${ordinal}` };
}
function offer(value: MotionBinding, target: CssPoint = { x: 600, y: 400 }, sequence = 1, ordinal = 1): MotionOffer {
  return { version: 1, type: 'offer', sequence, fence: fence(value, ordinal), activity: 'move_to', minimumActiveMs: 0, from: value.position,
    target, maximumSpeed: 100, acceleration: 500, validForMs: 10_000 };
}
function evidence(value: MotionBinding, proposal: MotionProposal, now: number, frameSequence = 1): RenderEvidence {
  return { fence: proposal.fence, frameSequence, now, position: proposal.position, viewport: value.viewport,
    posedBounds: { left: proposal.position.x - 20, top: proposal.position.y - 40, right: proposal.position.x + 20, bottom: proposal.position.y + 40 } };
}
function running(target: CssPoint = { x: 600, y: 400 }, value = binding()) {
  const follower = new WorldMotionFollower(), move = offer(value, target);
  expect(follower.bind(value, 0)).toBe(true);
  expect(follower.receive(move, 0)).toBe('accepted');
  expect(follower.beginAfterDelivered(move.fence, move.sequence, 0)).toBe(true);
  return { follower, value, move };
}
function commit(follower: WorldMotionFollower, value: MotionBinding, now: number, frameSequence: number): FrameObservation {
  const proposal = follower.sample(now, .016);
  expect(proposal).not.toBeNull();
  const observation = follower.afterSuccessfulRender(proposal!.token, evidence(value, proposal!, now, frameSequence));
  expect(observation).not.toBeNull();
  return observation!;
}

describe('strict local motion protocol', () => {
  it('accepts an actual asymmetric 700/100 envelope in 1080px while keeping symmetric metadata consistent',()=>{
    const value=binding({viewport:{width:1000,height:1080},position:{x:500,y:864},
      envelopeExtents:{left:40,top:700,right:60,bottom:100},maximumFootprint:{width:120,height:1400},
      safeCenters:{left:40,top:700,right:940,bottom:980}});
    expect(parseMotionBinding(value)).not.toBeNull();
    for(const target of [{x:40,y:864},{x:940,y:864},{x:500,y:700},{x:500,y:980}]){
      const {follower,move}=running(target,value),proposal=follower.sample(16,.016)!;
      expect(follower.snapshot().groundDistance).toBe(0);
      const e={...evidence(value,proposal,16),posedBounds:{left:proposal.position.x-40,top:proposal.position.y-700,right:proposal.position.x+60,bottom:proposal.position.y+100}};
      expect(follower.afterSuccessfulRender(proposal.token,e)).not.toBeNull();
      expect(follower.snapshot().groundDistance).toBeGreaterThan(0);
      expect(follower.snapshot().position).not.toEqual(move.target); // no target/arrival teleport
    }
  });
  it('rejects each asymmetric target outside its allowed edge and never banks a failed current hull',()=>{
    const value=binding({viewport:{width:1000,height:1080},position:{x:500,y:864},
      envelopeExtents:{left:40,top:700,right:60,bottom:100},maximumFootprint:{width:120,height:1400},
      safeCenters:{left:40,top:700,right:940,bottom:980}});
    for(const target of [{x:39.9,y:864},{x:940.1,y:864},{x:500,y:699.9},{x:500,y:980.1}]){
      const follower=new WorldMotionFollower();expect(follower.bind(value,0)).toBe(true);
      expect(follower.receive(offer(value,target),0)).toBe('rejected');expect(follower.sample(16,.016)).toBeNull();expect(follower.snapshot().groundDistance).toBe(0);
    }
    const {follower}=running({x:600,y:864},value),proposal=follower.sample(16,.016)!;
    const e={...evidence(value,proposal,16),posedBounds:{left:proposal.position.x-40,top:proposal.position.y-700.3,right:proposal.position.x+60,bottom:proposal.position.y+100}};
    expect(follower.afterSuccessfulRender(proposal.token,e)).toBeNull();expect(follower.snapshot().groundDistance).toBe(0);
  });
  it('does not relax one-edge overflow, malformed extents or symmetric metadata consistency',()=>{
    const base=binding({viewport:{width:1000,height:1080},position:{x:500,y:864},
      envelopeExtents:{left:40,top:700,right:60,bottom:100},maximumFootprint:{width:120,height:1400},
      safeCenters:{left:40,top:700,right:940,bottom:980}});
    for(const value of [binding({...base,safeCenters:{...base.safeCenters,left:39.9}}),binding({...base,safeCenters:{...base.safeCenters,top:699.9}}),
      binding({...base,safeCenters:{...base.safeCenters,right:940.1}}),binding({...base,safeCenters:{...base.safeCenters,bottom:980.1}}),
      binding({...base,envelopeExtents:{...base.envelopeExtents,top:-1}}),binding({...base,envelopeExtents:{...base.envelopeExtents,top:981},maximumFootprint:{width:120,height:1962}}),
      binding({...base,maximumFootprint:{width:120,height:800}})])expect(parseMotionBinding(value)).toBeNull();
  });
  it('copies and freezes valid binding/command data instead of retaining mutable wire objects', () => {
    const source = binding(), command = offer(source);
    const parsed = parseMotionBinding(source)!, parsedCommand = parseWorldMotionCommand(command)!;
    (source.position as { x: number }).x = 1;
    (command.target as { x: number }).x = 1;
    expect(parsed.position.x).toBe(500);
    expect(parsedCommand.type === 'offer' && parsedCommand.target.x).toBe(600);
    expect(Object.isFrozen(parsed.groundBasis.down)).toBe(true);
    expect(Object.isFrozen(parsedCommand.fence.actionId)).toBe(true);
  });
  it('rejects unbounded, nonfinite, unsafe, unsupported and unexpected payloads without invoking data getters', () => {
    for (const mutate of [
      (v: any) => v.actorId = '../other', (v: any) => v.viewport.width = Infinity,
      (v: any) => v.scale = NaN, (v: any) => v.layoutRevision = Number.MAX_SAFE_INTEGER + 1,
      (v: any) => v.groundBasis.down = { x: .02, z: 0 }, (v: any) => v.position.x = -1,
      (v: any) => v.unexpected = true, (v: any) => v.safeCenters.right = 1001,
    ]) {
      const value = structuredClone(binding()); mutate(value); expect(parseMotionBinding(value)).toBeNull();
    }
    for (const mutate of [
      (v: any) => v.sequence = 1.1, (v: any) => v.activity = 'shell', (v: any) => v.target.x = NaN,
      (v: any) => v.validForMs = 60_001, (v: any) => v.maximumSpeed = 501,
      (v: any) => v.fence.actionId.actorId = 'other', (v: any) => v.extra = true,
    ]) {
      const value = structuredClone(offer(binding())); mutate(value); expect(parseWorldMotionCommand(value)).toBeNull();
    }
    let calls = 0;
    const bad = { ...binding(), get scale() { calls++; return 1; } };
    expect(parseMotionBinding(bad)).toBeNull(); expect(calls).toBe(0);
    const terminal = { version: 1, type: 'terminal', sequence: 2, fence: fence(binding()), status: { toString() { throw new Error('must not invoke'); } } };
    expect(parseWorldMotionCommand(terminal)).toBeNull();
  });
  it('requires a finite frozen reference, matching envelope and separately bounded user and model scales', () => {
    const legacy = Object.fromEntries(Object.entries(binding()).filter(([key]) => key !== 'referenceRevision'));
    expect(parseMotionBinding(legacy)).toBeNull();
    for (const value of [
      binding({ referenceRevision: 0 }), binding({ referenceRevision: 1.5 }),
      binding({ referenceRevision: Number.MAX_SAFE_INTEGER + 1 }),
      binding({ referenceLocal: { x: 0, y: NaN, z: 0 } }),
      binding({ envelopeExtents: { left: -1, top: 40, right: 20, bottom: 40 } }),
      binding({ envelopeExtents: { left: 20, top: 40, right: 21, bottom: 40 } }),
      binding({ profileScale: .299 }), binding({ effectiveScale: 2.501 }),
      binding({ scale: .009 }), binding({ scale: 100.001 }),
    ]) expect(parseMotionBinding(value)).toBeNull();

    // Native user values and measured normalized-root scale are distinct units.
    const parsed = parseMotionBinding(binding({ profileScale: 2.5, effectiveScale: .3, scale: 100,
      envelopeExtents: { left: 0, top: 0, right: 20, bottom: 40 } }));
    expect(parsed).not.toBeNull();
    expect(parsed!.profileScale).toBe(2.5); expect(parsed!.effectiveScale).toBe(.3); expect(parsed!.scale).toBe(100);
    expect(Object.isFrozen(parsed!.referenceLocal)).toBe(true);
    expect(Object.isFrozen(parsed!.envelopeExtents)).toBe(true);
  });
});

describe('ordered native motion ownership', () => {
  it('cannot move before the exact native Delivered acceptance, including a late acceptance after Stop', () => {
    const follower = new WorldMotionFollower(), value = binding(), move = offer(value);
    follower.bind(value, 0); follower.receive(move, 0);
    expect(follower.sample(16, .016)).toBeNull();
    expect(follower.beginAfterDelivered({ ...move.fence, deliveryId: 'wrong' }, 1, 16)).toBe(false);
    expect(follower.beginAfterDelivered(move.fence, 2, 16)).toBe(false);
    follower.receive({ version: 1, type: 'terminal', sequence: 2, fence: move.fence, status: 'cancelled' }, 16);
    expect(follower.beginAfterDelivered(move.fence, 1, 16)).toBe(false);
    expect(follower.sample(32, .016)).toBeNull();
    expect(follower.snapshot().groundDistance).toBe(0);
  });
  it('does not restart a duplicate higher-sequence Offer or replace its pending rendered frame', () => {
    const { follower, move, value } = running();
    const proposal = follower.sample(16, .016)!;
    expect(follower.receive({ ...move, sequence: 2 }, 16)).toBe('duplicate');
    expect(follower.sample(16, .016)).toBe(proposal);
    const first = follower.afterSuccessfulRender(proposal.token, evidence(value, proposal, 16))!;
    expect(first.groundDistance).toBeGreaterThan(0);
    expect(follower.receive({ ...move, sequence: 3 }, 17)).toBe('duplicate');
    expect(follower.snapshot().groundDistance).toBe(first.groundDistance);
  });
  it('rejects conflicting targets for one identity and requires a fresh native action', () => {
    const { follower, move, value } = running();
    const old = follower.sample(16, .016)!;
    expect(follower.receive({ ...move, sequence: 2, target: { x: 400, y: 400 } }, 16)).toBe('rejected');
    expect(follower.snapshot().reason).toBe('conflicting_offer');
    expect(follower.afterSuccessfulRender(old.token, evidence(value, old, 16))).toBeNull();
    expect(follower.receive({ ...move, sequence: 3 }, 17)).toBe('stale');
    expect(follower.receive(offer(value, { x: 400, y: 400 }, 4, 2), 17)).toBe('accepted');
    expect(follower.snapshot().position).toEqual(value.position);
  });
  it('retains terminal ordering for reordered Offers and cannot let an older action clear a newer one', () => {
    const value = binding(), follower = new WorldMotionFollower(), old = offer(value, undefined, 9);
    follower.bind(value, 0);
    follower.receive({ version: 1, type: 'terminal', sequence: 10, fence: old.fence, status: 'cancelled' }, 0);
    expect(follower.receive(old, 0)).toBe('stale');
    expect(follower.receive({ ...old, sequence: 11 }, 0)).toBe('stale');
    const next = offer(value, undefined, 12, 2);
    expect(follower.receive(next, 0)).toBe('accepted');
    expect(follower.receive({ version: 1, type: 'terminal', sequence: 13, fence: old.fence, status: 'failed' }, 0)).toBe('accepted');
    expect(follower.beginAfterDelivered(next.fence, 12, 0)).toBe(true);
    expect(follower.sample(16, .016)?.walking).toBe(true);
  });
  it('invalidates superseded proposals without committing their position, speed or stride', () => {
    const { follower, value } = running();
    const pending = follower.sample(16, .016)!;
    const next = offer(value, { x: 400, y: 400 }, 2, 2);
    expect(follower.receive(next, 16)).toBe('accepted');
    expect(follower.afterSuccessfulRender(pending.token, evidence(value, pending, 16))).toBeNull();
    expect(follower.snapshot().groundDistance).toBe(0);
    expect(follower.snapshot().position).toEqual(value.position);
    expect(follower.beginAfterDelivered(next.fence, 2, 16)).toBe(true);
    expect(follower.sample(32, .016)!.travel.x).toBeLessThan(0);
  });
});

describe('applied eight-direction motion and successful-render barrier', () => {
  const directions = [
    [100, 0, 'right'], [100, 100, 'down_right'], [0, 100, 'down'], [-100, 100, 'down_left'],
    [-100, 0, 'left'], [-100, -100, 'up_left'], [0, -100, 'up'], [100, -100, 'up_right'],
  ] as const;
  it.each(directions)('turns and counts both ground components for %s,%s (%s)', (x, y, direction) => {
    const value = binding({ modelForwardYaw: .3 }), { follower } = running({ x: 500 + x, y: 400 + y }, value);
    const proposal = follower.sample(16, .016)!;
    expect(proposal.direction).toBe(direction);
    expect(proposal.desiredYaw).toBeCloseTo(Math.atan2(x * .01, y * .02) - .3, 10);
    expect(proposal.groundDistance).toBeCloseTo(Math.hypot(proposal.travel.x * .01, proposal.travel.y * .02), 10);
    expect(follower.snapshot().position).toEqual(value.position);
    expect(follower.snapshot().groundDistance).toBe(0);
    const observation = follower.afterSuccessfulRender(proposal.token, evidence(value, proposal, 16))!;
    expect(observation.direction).toBe(direction);
    expect(observation.totalGroundDistance).toBeCloseTo(proposal.groundDistance, 10);
    expect(observation.position).toEqual(proposal.position);
  });
  it('commits only the exact owning proposal token once and banks no failed render', () => {
    const { follower, value } = running();
    const pending = follower.sample(16, .016)!;
    expect(follower.afterSuccessfulRender({ frame: pending.token.frame }, evidence(value, pending, 16))).toBeNull();
    expect(follower.snapshot().groundDistance).toBe(0);
    expect(follower.renderFailed(pending.token)).toBe(true);
    expect(follower.afterSuccessfulRender(pending.token, evidence(value, pending, 16))).toBeNull();
    expect(follower.snapshot().position).toEqual(value.position);
    expect(follower.snapshot().groundDistance).toBe(0);
    expect(follower.sample(32, .016)).toBeNull();
  });
  it('counts actual applied displacement rather than a proposal when subpixel application differs', () => {
    const { follower, value } = running(), pending = follower.sample(16, .016)!;
    const measured = evidence(value, pending, 16);
    const actual = { x: value.position.x + pending.travel.x / 2, y: value.position.y };
    const observation = follower.afterSuccessfulRender(pending.token, { ...measured, position: actual })!;
    expect(observation.position).toEqual(actual);
    expect(observation.groundDistance).toBeCloseTo(pending.groundDistance / 2, 10);
    expect(follower.snapshot().groundDistance).toBeCloseTo(pending.groundDistance / 2, 10);
  });
  it('does not turn a leaning body outside its frozen anchor into semantic travel', () => {
    const { follower, value } = running();
    for (const [index, offset] of [5, -15, 3].entries()) {
      const now = (index + 1) * 16, pending = follower.sample(now, 0)!;
      const body = { left: pending.position.x + offset, right: pending.position.x + offset + 10,
        top: pending.position.y + 5, bottom: pending.position.y + 15 };
      const observation = follower.afterSuccessfulRender(pending.token, {
        ...evidence(value, pending, now, index + 1), posedBounds: body,
      });
      expect(observation).not.toBeNull();
      expect(observation!.position).toEqual(value.position);
      expect(observation!.groundDistance).toBe(0);
      expect(observation!.walking).toBe(false);
      expect(observation!.targetIdleFrames).toBe(0);
    }
    expect(follower.snapshot().position).toEqual(value.position);
    expect(follower.snapshot().groundDistance).toBe(0);
    expect(follower.snapshot().phase).toBe('running');
  });
  it.each(['left', 'top', 'right', 'bottom'] as const)('cancels a %s envelope breach without enlarging the accepted calibration', edge => {
    const { follower, value } = running(), pending = follower.sample(16, .016)!;
    const bounds = { left: pending.position.x - 5, top: pending.position.y - 5,
      right: pending.position.x + 5, bottom: pending.position.y + 5 };
    const limit = edge === 'left' ? pending.position.x - value.envelopeExtents.left
      : edge === 'right' ? pending.position.x + value.envelopeExtents.right
        : edge === 'top' ? pending.position.y - value.envelopeExtents.top
          : pending.position.y + value.envelopeExtents.bottom;
    bounds[edge] = limit + (edge === 'left' || edge === 'top' ? -.251 : .251);
    expect(follower.afterSuccessfulRender(pending.token, { ...evidence(value, pending, 16), posedBounds: bounds })).toBeNull();
    expect(follower.snapshot().reason).toBe('invalid_render');
    expect(follower.snapshot().position).toEqual(value.position);
    expect(follower.snapshot().groundDistance).toBe(0);
    expect(follower.afterSuccessfulRender(pending.token, evidence(value, pending, 17, 2))).toBeNull();
    expect(follower.sample(32, .016)).toBeNull();
    expect(value.envelopeExtents).toEqual({ left: 20, top: 40, right: 20, bottom: 40 });
  });
  it('uses the already inverse-scaled ground basis without applying user or model scale twice', () => {
    const value = binding({ profileScale: 2.5, effectiveScale: .3, scale: 10 });
    const { follower } = running({ x: 600, y: 500 }, value), pending = follower.sample(16, .016)!;
    const observation = follower.afterSuccessfulRender(pending.token, evidence(value, pending, 16))!;
    expect(observation.groundDistance).toBeCloseTo(Math.hypot(observation.travel.x * .01, observation.travel.y * .02), 12);
    expect(observation.groundDistance).toBeGreaterThan(0);
    expect(observation.totalGroundDistance).toBe(observation.groundDistance);
  });
  it('rejects mismatched/clipped/unmeasured render evidence without claiming travel', () => {
    for (const mutate of [
      (v: any) => v.position.x += 10, (v: any) => v.viewport.width++,
      (v: any) => v.posedBounds.left = -1, (v: any) => v.posedBounds.right += 50,
      (v: any) => v.posedBounds = { left: 0, top: 0, right: 10, bottom: 10 },
      (v: any) => v.frameSequence = 0, (v: any) => v.now = 300,
    ]) {
      const { follower, value } = running(), pending = follower.sample(16, .016)!;
      const measured = structuredClone(evidence(value, pending, 16)); mutate(measured);
      expect(follower.afterSuccessfulRender(pending.token, measured)).toBeNull();
      expect(follower.snapshot().groundDistance).toBe(0);
      expect(follower.snapshot().position).toEqual(value.position);
    }
  });
  it('reaching the mathematical target produces only later stable frame observations, never autonomous completion', () => {
    const { follower, value, move } = running({ x: 502, y: 400 });
    let last: FrameObservation | null = null;
    for (let frame = 1; frame <= 100; frame++) {
      last = commit(follower, value, frame * 16, frame);
      if (last.targetIdleFrames === 2) break;
    }
    expect(last!.targetIdleFrames).toBe(2);
    expect(last!.walking).toBe(false);
    expect(last!.groundDistance).toBe(0);
    expect(follower.snapshot().phase).toBe('at_target');
    expect(follower.sample(1700, .016)).toBeNull(); // a missing frame stream cancels, not completes
    expect(follower.snapshot().reason).toBe('frame_gap');
    expect(follower.receive({ version: 1, type: 'terminal', sequence: 2, fence: move.fence, status: 'completed' }, 1700)).toBe('accepted');
    expect(follower.snapshot().phase).toBe('idle');
  });
  it('uses bounded per-frame motion with no sleep or backward-clock catch-up', () => {
    const { follower, value } = running();
    const pending = follower.sample(200, 1)!;
    expect(pending.cssDistance).toBeLessThanOrEqual(100 * WORLD_MAX_FRAME_SECONDS);
    follower.afterSuccessfulRender(pending.token, evidence(value, pending, 200));
    const distance = follower.snapshot().groundDistance;
    const backwards = follower.sample(100, .016)!;
    expect(backwards.cssDistance).toBe(0);
    follower.afterSuccessfulRender(backwards.token, evidence(value, backwards, 100, 2));
    expect(follower.snapshot().groundDistance).toBe(distance);
    expect(follower.sample(1000, .016)).toBeNull();
    expect(follower.snapshot().groundDistance).toBe(distance);
  });
});

describe('pause and render-binding invalidation', () => {
  it('pause invalidates old pending frames and resume needs a newer exact native command', () => {
    const { follower, value, move } = running(), old = follower.sample(16, .016)!;
    expect(follower.receive({ version: 1, type: 'pause', sequence: 2, fence: move.fence }, 16)).toBe('accepted');
    expect(follower.afterSuccessfulRender(old.token, evidence(value, old, 16))).toBeNull();
    const paused = follower.sample(32, .016)!;
    expect(paused.walking).toBe(false);
    expect(follower.afterSuccessfulRender(paused.token, evidence(value, paused, 32))?.groundDistance).toBe(0);
    expect(follower.receive({ version: 1, type: 'resume', sequence: 2, fence: move.fence }, 1000)).toBe('stale');
    expect(follower.receive({ version: 1, type: 'resume', sequence: 3, fence: move.fence }, 1000)).toBe('accepted');
    expect(follower.sample(1016, .016)!.cssDistance).toBeLessThan(1);
  });
  it.each(['assetRevision', 'layoutRevision', 'measurementRevision', 'topologyRevision', 'referenceRevision'] as const)('requires a new registration after %s changes and rejects old frame callbacks', key => {
    const { follower, value, move } = running(), pending = follower.sample(16, .016)!;
    const next = binding({ ...value, bindingSequence: 2, [key]: 2, scale: 1.5 });
    expect(follower.bind(next, 16)).toBe(true);
    expect(follower.afterSuccessfulRender(pending.token, evidence(value, pending, 16))).toBeNull();
    expect(follower.receive({ ...move, sequence: 100 }, 16)).toBe('stale');
    expect(follower.receive(offer(next, undefined, 101, 1), 16)).toBe('stale');
    expect(follower.receive(offer(next, undefined, 102, 2), 16)).toBe('accepted');
    expect(follower.snapshot().groundDistance).toBe(0);
  });
  it('cannot use a newer reference fence to resume a paused action or let a stale frame poison a fresh one', () => {
    const { follower, value, move } = running(), prePause = follower.sample(16, .016)!;
    expect(follower.receive({ version: 1, type: 'pause', sequence: 2, fence: move.fence }, 16)).toBe('accepted');
    for (let frame = 1; frame <= 5; frame++) {
      const now = frame * 100, pending = follower.sample(now, .1)!;
      const observation = follower.afterSuccessfulRender(pending.token, evidence(value, pending, now, frame))!;
      expect(observation.walking).toBe(false); expect(observation.groundDistance).toBe(0);
    }
    expect(follower.receive({ version: 1, type: 'resume', sequence: 999,
      fence: { ...move.fence, referenceRevision: 2 } }, 500)).toBe('stale');
    expect(follower.afterSuccessfulRender(prePause.token, evidence(value, prePause, NaN, 999))).toBeNull();
    expect(follower.receive({ version: 1, type: 'resume', sequence: 3, fence: move.fence }, 600)).toBe('accepted');
    const fresh = follower.sample(616, .016)!;
    expect(fresh.cssDistance).toBeGreaterThan(0); expect(fresh.cssDistance).toBeLessThan(1);
    expect(follower.afterSuccessfulRender(fresh.token, evidence(value, fresh, 616, 6))).not.toBeNull();
    expect(follower.receive({ version: 1, type: 'terminal', sequence: 4, fence: move.fence, status: 'cancelled' }, 632)).toBe('accepted');
    expect(follower.receive({ version: 1, type: 'resume', sequence: 5, fence: move.fence }, 632)).toBe('stale');
    expect(follower.sample(648, .016)).toBeNull();
  });
  it('an obsolete frame callback cannot poison a replacement with its invalid timestamp', () => {
    const { follower, value } = running(), old = follower.sample(16, .016)!;
    const next = binding({ ...value, renderSessionId: 'render-2', bindingSequence: 2, layoutRevision: 2 });
    follower.bind(next, 16);
    const move = offer(next, undefined, 1, 2); follower.receive(move, 16);
    follower.beginAfterDelivered(move.fence, 1, 16);
    const current = follower.sample(32, .016)!;
    expect(follower.afterSuccessfulRender(old.token, { ...evidence(value, old, NaN), frameSequence: 999 })).toBeNull();
    expect(follower.afterSuccessfulRender(current.token, evidence(next, current, 32))).not.toBeNull();
    expect(follower.snapshot().phase).toBe('running');
  });
  it('reconnects without replay and a disposed attachment cannot be rebound by late callbacks', () => {
    const { follower, value, move } = running();
    follower.invalidate();
    const next = binding({ ...value, renderSessionId: 'render-2', bindingSequence: 2 });
    expect(follower.bind(next, 1000)).toBe(true);
    expect(follower.receive({ ...move, sequence: 999 }, 1000)).toBe('stale');
    expect(follower.receive(offer(next, undefined, 1, 1), 1000)).toBe('stale');
    expect(follower.receive(offer(next, undefined, 2, 2), 1000)).toBe('accepted');
    follower.dispose();
    expect(follower.bind({ ...next, bindingSequence: 3 }, 1000)).toBe(false);
  });
  it('reduced motion or unavailable walk support never selects teleportation or invented gait', () => {
    for (const value of [binding({ reducedMotion: true }), binding({ walkSupported: false, stationSupported: false })]) {
      const follower = new WorldMotionFollower(); follower.bind(value, 0);
      expect(follower.receive(offer(value), 0)).toBe('rejected');
      expect(follower.sample(16, .016)).toBeNull();
      expect(follower.snapshot().position).toEqual(value.position);
      expect(follower.snapshot().groundDistance).toBe(0);
    }
  });
  it('isolates another actor/action/output fence and refuses out-of-safe-area targets without clamping', () => {
    const { follower, value, move } = running();
    const wrongActor = { ...move, sequence: 2, fence: { ...move.fence, actorId: 'other', actionId: { ...move.fence.actionId, actorId: 'other' } } };
    expect(follower.receive(wrongActor, 0)).toBe('stale');
    expect(follower.receive({ ...move, sequence: 2, fence: { ...move.fence, renderSessionId: 'other-render' } }, 0)).toBe('stale');
    const bad = offer(value, { x: 0, y: 0 }, 2, 2);
    expect(follower.receive(bad, 0)).toBe('rejected');
    expect(follower.snapshot().position).toEqual(value.position);
    expect(follower.snapshot().groundDistance).toBe(0);
  });
});
