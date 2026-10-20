import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { MToonMaterial } from '@pixiv/three-vrm';
import { configureDesktopCamera } from './desktop-framing';
import { containsPosedEnvelope } from './world-motion';
import { mapReachableCanonical, CONSERVATIVE_ENCLOSURE_KIND, WorldRenderGeometry, GEOMETRY_MAX_NODES, GEOMETRY_MAX_VERTEX_VISITS, GEOMETRY_MAX_DRAW_GROUPS, GEOMETRY_MAX_DEFORM_OPERATIONS,
  type GeometryBinding, type GeometryFrame, type GeometryResult, type RenderPort, type SurfaceObservation } from './world-render-geometry';

// Real Three CPU geometry/morph/skin/mixer, with a SYNTHETIC draw callback port.
// These tests neither create WebGL nor prove pixels, native visibility or arrival.
const binding = (patch: Partial<GeometryBinding> = {}): GeometryBinding => ({ actorId: 'nova', sourceWorldId: 'life-nova',
  renderSessionId: 'render-1', bindingSequence: 1, worldInstanceId: 'world-1', stageId: 'stage-1', configRevision: 1,
  topologyRevision: 1, layoutRevision: 1, assetRevision: 1, measurementRevision: 1, referenceRevision: 1, scale: 1, ...patch });
function triangle() {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute([-.5, 0, 0, .5, 0, 0, 0, 1, 0], 3));
  return geometry;
}
function fixture(mesh: THREE.Object3D = new THREE.Mesh(triangle(), new THREE.MeshBasicMaterial()), enclosureKind?:typeof CONSERVATIVE_ENCLOSURE_KIND) {
  const scene = new THREE.Scene(), root = new THREE.Group(); root.add(mesh); scene.add(root);
  const camera = new THREE.OrthographicCamera(-10, 10, 5, -5, .05, 100); configureDesktopCamera(camera);
  const probe = new WorldRenderGeometry(enclosureKind); let frame: GeometryFrame = { binding: binding(), frameSequence: 1 };
  let surface: SurfaceObservation = { connected: true, visible: true, viewport: { width: 200, height: 100 }, clip: { left: 0, top: 0, right: 200, bottom: 100 } };
  const state = { lost: false, draw: true, throwAfterDraw: false, scissor: false, target: false, renders: 0,
    beforeDraw: () => {}, afterDraw: () => {}, afterSubmittedDraw: (_mesh:THREE.Mesh,_draw:number) => {},
    viewport: new THREE.Vector4(0, 0, 200, 100), size: new THREE.Vector2(200, 100) };
  const port: RenderPort = {
    getContext: () => ({ isContextLost: () => state.lost }), getScissorTest: () => state.scissor,
    getRenderTarget: () => state.target ? new THREE.WebGLRenderTarget(1, 1) : null,
    getSize: target => target.copy(state.size), getViewport: target => target.copy(state.viewport),
    render(s, c) {
      state.renders++; state.beforeDraw(); s.updateMatrixWorld(true); c.updateMatrixWorld(true);
      if (state.draw) {
        let submitted=0;
        const visit = (node: THREE.Object3D) => {
          if (!node.visible) return;
          if (node instanceof THREE.Mesh && c.layers.test(node.layers)) {
            const materials = node.material;
            const entries = Array.isArray(materials)
              ? node.geometry.groups.map((group: THREE.GeometryGroup) => ({ material: materials[group.materialIndex!], group }))
              : [{ material: materials, group: null }];
            for (const { material, group } of entries) if (material?.visible) {
              node.onAfterRender(port as unknown as THREE.WebGLRenderer, s as THREE.Scene, c, node.geometry, material, group as unknown as Parameters<THREE.Mesh['onAfterRender']>[5]);
              state.afterSubmittedDraw(node,++submitted);
            }
          }
          for (const child of node.children) visit(child);
        };
        visit(root);
      }
      state.afterDraw(); if (state.throwAfterDraw) throw new Error('synthetic render failure');
    },
  };
  expect(probe.bind(frame.binding, root, camera, new THREE.Matrix4())).toBe(true);
  return { probe, root, scene, camera, state, mesh, port,
    frame: () => frame, setFrame: (next: GeometryFrame) => { frame = next; }, surface: () => surface,
    setSurface: (next: SurfaceObservation) => { surface = next; },
    run: (clock = () => 0, options:Readonly<{allowCoveredPreparation?:boolean}> = {}) => probe.renderFrame(frame, scene, port, () => surface, () => frame, clock, options),
  };
}
function measurement(result: GeometryResult) {
  expect(result.available).toBe(true);
  if (!result.available) throw new Error(result.reason);
  return result.measurement;
}

describe('post-draw posed geometry (synthetic render port)', () => {
  it('publishes immutable current posed bounds only after an actual submitted callback', () => {
    const f = fixture(); const m = measurement(f.run());
    expect(m.posedBounds.left).toBeCloseTo(95); expect(m.posedBounds.right).toBeCloseTo(105);
    expect(m.maximumFootprint.width).toBeCloseTo(10); expect(m.meshes).toBe(1); expect(m.vertices).toBe(3);
    expect(Object.isFrozen(m)).toBe(true); expect(Object.isFrozen(m.posedBounds)).toBe(true);
    expect((f.mesh as THREE.Mesh).onAfterRender).toBe(THREE.Object3D.prototype.onAfterRender);
    const absent = fixture(); absent.state.draw = false;
    expect(absent.run()).toEqual({ available: false, reason: 'empty_geometry' });
  });

  it.each(['basic', 'MToon'] as const)('reads current %s morph and skeletal animation, ignoring stale cached neutral boxes', kind => {
    const geometry = triangle(); geometry.morphTargetsRelative = true;
    geometry.morphAttributes.position = [new THREE.Float32BufferAttribute([0, 0, 0, 0, 0, 0, 0, 1, 0], 3)];
    geometry.setAttribute('skinIndex', new THREE.Uint16BufferAttribute(new Array(12).fill(0), 4));
    geometry.setAttribute('skinWeight', new THREE.Float32BufferAttribute([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0], 4));
    const mesh = new THREE.SkinnedMesh(geometry, kind === 'MToon' ? new MToonMaterial() : new THREE.MeshBasicMaterial());
    const bone = new THREE.Bone(); bone.name = 'movingBone'; mesh.add(bone); mesh.bind(new THREE.Skeleton([bone]));
    mesh.computeBoundingBox(); const oldBox = mesh.boundingBox!.clone();
    const f = fixture(mesh); const before = measurement(f.run());
    const clip = new THREE.AnimationClip('authored pose', 1, [
      new THREE.NumberKeyframeTrack('.morphTargetInfluences[0]', [0, 1], [0, 1]),
      new THREE.VectorKeyframeTrack('movingBone.position', [0, 1], [0, 0, 0, .25, 0, 0]),
    ]);
    const mixer = new THREE.AnimationMixer(mesh); const action = mixer.clipAction(clip); action.setLoop(THREE.LoopOnce, 1); action.clampWhenFinished = true; action.play();
    mixer.update(1); f.setFrame({ ...f.frame(), frameSequence: 2 }); const after = measurement(f.run());
    expect(mesh.boundingBox!.equals(oldBox)).toBe(true);
    expect(after.posedBounds.left - before.posedBounds.left).toBeCloseTo(2.5);
    expect(after.posedBounds.top).toBeLessThan(before.posedBounds.top - 8);
    expect(after.maximumFootprint.height).toBeGreaterThan(before.maximumFootprint.height);
  });

  it('honours submitted material groups, visibility, camera layers and drawRange', () => {
    const geometry = triangle(); geometry.setAttribute('position', new THREE.Float32BufferAttribute([
      -.5, 0, 0, .5, 0, 0, 0, 1, 0, 1000, 0, 0, 1001, 0, 0, 1000, 1, 0,
    ], 3)); geometry.addGroup(0, 3, 0); geometry.addGroup(3, 3, 1); geometry.setDrawRange(0, 3);
    const mesh = new THREE.Mesh(geometry, [new THREE.MeshBasicMaterial(), new THREE.MeshBasicMaterial({ visible: false })]);
    const hidden = new THREE.Mesh(new THREE.BoxGeometry(1000, 1000, 1000), new THREE.MeshBasicMaterial()); hidden.visible = false; mesh.add(hidden);
    const wrongLayer = new THREE.Mesh(hidden.geometry, hidden.material); wrongLayer.layers.set(2); mesh.add(wrongLayer);
    expect(measurement(fixture(mesh).run()).vertices).toBe(3);
  });

  it('evaluates each indexed vertex once per mesh/frame across overlapping submitted material groups',()=>{
    const geometry=triangle();geometry.setIndex([0,1,2,0,1,2]);geometry.addGroup(0,6,0);geometry.addGroup(0,6,1);
    const mesh=new THREE.Mesh(geometry,[new THREE.MeshBasicMaterial(),new THREE.MeshStandardMaterial()]);
    const f=fixture(mesh),before=measurement(f.run()),unindexed=measurement(fixture().run());
    expect(before.vertices).toBe(3);expect(before.posedBounds).toEqual(unindexed.posedBounds);
    expect(before.envelopeExtents).toEqual(unindexed.envelopeExtents);expect(before.maximumFootprint).toEqual(unindexed.maximumFootprint);
    mesh.position.x=.5;f.setFrame({...f.frame(),frameSequence:2});const after=measurement(f.run());
    expect(after.vertices).toBe(3);expect(after.posedBounds.left-before.posedBounds.left).toBeCloseTo(5);
  });
  it('keeps shared geometry evaluations isolated between differently posed meshes',()=>{
    const geometry=triangle();geometry.setIndex([0,1,2,0,1,2]);const group=new THREE.Group();
    const a=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial()),b=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial());b.position.x=2;group.add(a,b);
    const m=measurement(fixture(group).run());expect(m.vertices).toBe(6);expect(m.meshes).toBe(2);
    expect(m.posedBounds.left).toBeCloseTo(95);expect(m.posedBounds.right).toBeCloseTo(125);
  });
  it('re-evaluates indexed morph and skin deformation from the next actual mixer pose',()=>{
    const geometry=triangle();geometry.setIndex([0,1,2,0,1,2]);geometry.morphTargetsRelative=true;
    geometry.morphAttributes.position=[new THREE.Float32BufferAttribute([0,0,0,0,0,0,0,1,0],3)];
    geometry.setAttribute('skinIndex',new THREE.Uint16BufferAttribute(new Array(12).fill(0),4));
    geometry.setAttribute('skinWeight',new THREE.Float32BufferAttribute([1,0,0,0,1,0,0,0,1,0,0,0],4));
    const mesh=new THREE.SkinnedMesh(geometry,new THREE.MeshBasicMaterial()),bone=new THREE.Bone();bone.name='Bone';mesh.add(bone);mesh.bind(new THREE.Skeleton([bone]));
    const f=fixture(mesh),before=measurement(f.run());
    const clip=new THREE.AnimationClip('move',1,[new THREE.NumberKeyframeTrack('.morphTargetInfluences[0]',[0,1],[0,1]),
      new THREE.VectorKeyframeTrack('Bone.position',[0,1],[0,0,0,.25,0,0])]);
    const mixer=new THREE.AnimationMixer(mesh),action=mixer.clipAction(clip).setLoop(THREE.LoopOnce,1);action.clampWhenFinished=true;action.play();mixer.update(1);
    f.setFrame({...f.frame(),frameSequence:2});const after=measurement(f.run());expect(before.vertices).toBe(3);expect(after.vertices).toBe(3);
    expect(after.posedBounds.left-before.posedBounds.left).toBeCloseTo(2.5);expect(after.posedBounds.top).toBeLessThan(before.posedBounds.top-8);
  });
  it('still validates an unsupported later material even when all its vertex indices were already evaluated',()=>{
    const geometry=triangle();geometry.setIndex([0,1,2]);geometry.addGroup(0,3,0);geometry.addGroup(0,3,1);
    const mesh=new THREE.Mesh(geometry,[new THREE.MeshBasicMaterial(),new THREE.ShaderMaterial()]);
    expect(fixture(mesh).run()).toEqual({available:false,reason:'unsupported_deformation'});
  });
  it.each(['matrix','morph','skin','attribute'] as const)('rejects a cached %s pose change between submitted groups',mode=>{
    const geometry=triangle();geometry.setIndex([0,1,2]);geometry.addGroup(0,3,0);geometry.addGroup(0,3,1);
    geometry.morphTargetsRelative=true;geometry.morphAttributes.position=[new THREE.Float32BufferAttribute([0,0,0,0,0,0,0,1,0],3)];
    geometry.setAttribute('skinIndex',new THREE.Uint16BufferAttribute(new Array(12).fill(0),4));
    geometry.setAttribute('skinWeight',new THREE.Float32BufferAttribute([1,0,0,0,1,0,0,0,1,0,0,0],4));
    const mesh=new THREE.SkinnedMesh(geometry,[new THREE.MeshBasicMaterial(),new THREE.MeshBasicMaterial()]),bone=new THREE.Bone();mesh.add(bone);mesh.bind(new THREE.Skeleton([bone]));
    const f=fixture(mesh);f.state.afterSubmittedDraw=(_mesh,draw)=>{if(draw!==1)return;
      if(mode==='matrix')mesh.position.x=.5;if(mode==='morph')mesh.morphTargetInfluences![0]=1;
      if(mode==='skin')bone.position.x=.5;if(mode==='attribute'){geometry.attributes.position.setX(0,-.75);geometry.attributes.position.needsUpdate=true;}
      mesh.updateMatrixWorld(true);
    };
    expect(f.run()).toEqual({available:false,reason:'invalid_geometry'});
  });
  it('also rejects a pose changed after the final submitted group before publishing geometry',()=>{
    const geometry=triangle();geometry.setIndex([0,1,2,0,1,2]);const mesh=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial()),f=fixture(mesh);
    f.state.afterDraw=()=>{mesh.position.x=.5;mesh.updateMatrixWorld(true);};
    expect(f.run()).toEqual({available:false,reason:'invalid_geometry'});
  });
  it('counts repeated indices as raw work while the 200k cap counts actual vertex evaluations',()=>{
    const geometry=triangle(),entries=Math.ceil((GEOMETRY_MAX_VERTEX_VISITS+1)/3)*3;
    geometry.setIndex(Array.from({length:entries},(_,i)=>i%3));
    expect(measurement(fixture(new THREE.Mesh(geometry,new THREE.MeshBasicMaterial())).run()).vertices).toBe(3);
    geometry.setIndex(new THREE.BufferAttribute(new Uint32Array(GEOMETRY_MAX_DEFORM_OPERATIONS+1),1));
    expect(fixture(new THREE.Mesh(geometry,new THREE.MeshBasicMaterial())).run()).toEqual({available:false,reason:'measurement_budget'});
  });
  it('checks the 8ms CPU clock throughout duplicate-index scans even when no new vertex is evaluated',()=>{
    const geometry=triangle();geometry.setIndex(Array.from({length:3000},(_,i)=>i%3));const f=fixture(new THREE.Mesh(geometry,new THREE.MeshBasicMaterial()));let reads=0;
    expect(f.run(()=>f.state.renders>0?++reads:0)).toEqual({available:false,reason:'measurement_budget'});
  });

  it('requires a newer actual draw after covered rigid preparation and preserves exact submitted bounds',()=>{
    const geometry=new THREE.SphereGeometry(.5,16,8),mesh=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial()),f=fixture(mesh);
    const expected=measurement(fixture(new THREE.Mesh(geometry.clone(),new THREE.MeshBasicMaterial())).run());
    expect(f.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});expect(f.state.renders).toBe(1);
    f.setFrame({...f.frame(),frameSequence:2});const actual=measurement(f.run());expect(f.state.renders).toBe(2);
    expect(actual.posedBounds).toEqual(expected.posedBounds);expect(actual.envelopeExtents).toEqual(expected.envelopeExtents);
    expect(actual.referenceLocal).toEqual(expected.referenceLocal);
  });
  it('cooperatively prepares without partial measurements, then rejects changed active source without restarting counters',()=>{
    const geometry=new THREE.SphereGeometry(.5,32,16),mesh=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial()),f=fixture(mesh);let clock=0;
    expect(f.run(()=>clock+=.01,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});
    f.setFrame({...f.frame(),frameSequence:2});expect(f.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});
    f.setFrame({...f.frame(),frameSequence:3});expect(f.run().available).toBe(true);
    geometry.attributes.position.needsUpdate=true;f.setFrame({...f.frame(),frameSequence:4});
    expect(f.run()).toEqual({available:false,reason:'invalid_geometry'});
    f.setFrame({...f.frame(),frameSequence:5});expect(f.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,reason:'invalid_geometry'});
    const rebound={binding:binding({bindingSequence:2,assetRevision:2}),frameSequence:6};expect(f.probe.bind(rebound.binding,f.root,f.camera,new THREE.Matrix4())).toBe(true);f.setFrame(rebound);
    expect(f.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});
  });
  it('does not let an already-ready expensive query starve a later pending range on covered frames',()=>{
    const root=new THREE.Group(),readyMesh=new THREE.Mesh(new THREE.SphereGeometry(.5,16,8),new THREE.MeshBasicMaterial()),
      laterMesh=new THREE.Mesh(new THREE.SphereGeometry(.4,32,16),new THREE.MeshBasicMaterial());
    laterMesh.visible=false;root.add(readyMesh,laterMesh);const f=fixture(root);
    expect(f.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});
    f.setFrame({...f.frame(),frameSequence:2});expect(f.run().available).toBe(true);
    const rigid=(f.probe as unknown as {rigid:{query:(...args:any[])=>any}}).rigid,query=rigid.query.bind(rigid);
    let clock=0;const queried:THREE.Mesh[]=[];
    rigid.query=(...args:any[])=>{queried.push(args[0]);if(args[0]===readyMesh)clock+=4.1;return query(...args);};
    laterMesh.visible=true;f.setFrame({...f.frame(),frameSequence:3});
    expect(f.run(()=>clock,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});expect(queried).toEqual([readyMesh]);
    clock=0;queried.length=0;f.setFrame({...f.frame(),frameSequence:4});
    expect(f.run(()=>clock,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});expect(queried).toHaveLength(0);
    clock=0;f.setFrame({...f.frame(),frameSequence:5});expect(f.run(()=>clock).available).toBe(true);
    expect(queried).toEqual([readyMesh,laterMesh]);expect(f.state.renders).toBe(5);
  });

  it('keeps a fresh skin/morph path exact while covered rigid preparation is enabled',()=>{
    const geometry=triangle();geometry.setIndex(Array.from({length:99},(_,i)=>i%3));geometry.morphTargetsRelative=true;
    geometry.morphAttributes.position=[new THREE.Float32BufferAttribute([0,0,0,0,0,0,0,1,0],3)];
    const mesh=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial());mesh.morphTargetInfluences![0]=1;
    const f=fixture(mesh),m=measurement(f.run(()=>0,{allowCoveredPreparation:true}));expect(m.vertices).toBe(3);
    expect(m.posedBounds.top).toBeLessThan(measurement(fixture().run()).posedBounds.top-8);
  });
  it('never silently prepares a new large range after covered preparation was disabled',()=>{
    const geometry=new THREE.SphereGeometry(.5,16,8),mesh=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial()),f=fixture(mesh);
    expect(f.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});
    f.setFrame({...f.frame(),frameSequence:2});expect(f.run().available).toBe(true);
    geometry.setDrawRange(0,geometry.index!.count-3);f.setFrame({...f.frame(),frameSequence:3});
    expect(f.run()).toEqual({available:false,reason:'invalid_geometry'});
  });

  it('keeps alpha-cutout geometry conservative without claiming visible pixels', () => {
    const mesh = new THREE.Mesh(triangle(), new THREE.MeshBasicMaterial({ alphaTest: .9, transparent: true }));
    const m = measurement(fixture(mesh).run()); expect(m.maximumFootprint.width).toBeCloseTo(10);
    expect(Object.hasOwn(m, 'visiblePixels')).toBe(false); expect(Object.hasOwn(m, 'arrived')).toBe(false);
  });

  it('derives both inverse camera/ground axes from actual orthographic matrices and model scale', () => {
    const f = fixture(); const first = measurement(f.run());
    expect(first.groundBasis.right.x).toBeCloseTo(.1); expect(first.groundBasis.right.z).toBeCloseTo(0);
    expect(first.groundBasis.down.x).toBeCloseTo(0); expect(first.groundBasis.down.z).toBeCloseTo(.2);
    const frame = { binding: binding({ bindingSequence: 2, measurementRevision: 2, scale: 2 }), frameSequence: 2 };
    expect(f.probe.bind(frame.binding, f.root, f.camera, new THREE.Matrix4().makeScale(2, 2, 2))).toBe(true); f.setFrame(frame);
    const next = measurement(f.run()); expect(next.groundBasis.right.x).toBeCloseTo(.05); expect(next.groundBasis.down.z).toBeCloseTo(.1);
  });
  it('keeps an explicit reference outside a leaning body fixed while measuring current, separate extents', () => {
    const f = fixture(), mesh = f.mesh as THREE.Mesh;
    const frame = { binding: binding({ bindingSequence: 2, referenceRevision: 2 }), frameSequence: 1 };
    const reference = new THREE.Vector3(2, .5, 0);
    expect(f.probe.bind(frame.binding, f.root, f.camera, new THREE.Matrix4(), reference)).toBe(true);
    f.setFrame(frame); reference.set(100, 100, 100); // Caller mutation cannot move the frozen point.
    const initial = measurement(f.run());
    expect(initial.position.x).toBeCloseTo(120);
    expect(initial.position.x).toBeGreaterThan(initial.posedBounds.right);
    expect(initial.referenceLocal).toEqual({ x: 2, y: .5, z: 0 });
    expect(initial.envelopeExtents.left).toBeCloseTo(25); expect(initial.envelopeExtents.right).toBe(0);
    expect(initial.maximumFootprint.width).toBeCloseTo(50);

    mesh.position.x = 1; f.setFrame({ ...frame, frameSequence: 2 });
    const leaned = measurement(f.run());
    expect(leaned.position).toEqual(initial.position);
    expect(leaned.posedBounds.left - initial.posedBounds.left).toBeCloseTo(10);
    expect(containsPosedEnvelope(leaned.position, leaned.posedBounds, initial.envelopeExtents, 1e-7)).toBe(true);

    mesh.position.x = 3; f.setFrame({ ...frame, frameSequence: 3 });
    const breached = measurement(f.run());
    expect(breached.position).toEqual(initial.position);
    expect(breached.position.x).toBeLessThan(breached.posedBounds.left);
    expect(breached.envelopeExtents.left).toBe(0); expect(breached.envelopeExtents.right).toBeCloseTo(15);
    expect(containsPosedEnvelope(breached.position, breached.posedBounds, initial.envelopeExtents, 1e-7)).toBe(false);
    expect(initial.envelopeExtents.left).toBeCloseTo(25); expect(initial.envelopeExtents.right).toBe(0);
    expect(Object.isFrozen(initial.envelopeExtents)).toBe(true);
  });
  it('requires the exact reference revision and never promotes an old reference frame after rebind', () => {
    const f = fixture(), oldFrame = f.frame();
    const changed = { binding: binding({ bindingSequence: 2, referenceRevision: 2 }), frameSequence: 1 };
    expect(f.probe.bind(changed.binding, f.root, f.camera, new THREE.Matrix4(), new THREE.Vector3(-2, .5, 0))).toBe(true);
    f.setFrame(oldFrame);
    expect(f.run()).toEqual({ available: false, reason: 'stale_frame' });
    expect(f.state.renders).toBe(0);
    f.setFrame(changed); f.state.afterDraw = () => f.setFrame({ ...changed,
      binding: { ...changed.binding, referenceRevision: 1 } });
    expect(f.run()).toEqual({ available: false, reason: 'stale_frame' });
    f.state.afterDraw = () => {}; f.setFrame({ ...changed, frameSequence: 2 });
    const fresh = measurement(f.run());
    expect(fresh.position.x).toBeCloseTo(80);
    expect(fresh.frame.binding.referenceRevision).toBe(2);
    expect(fresh.referenceLocal).toEqual({ x: -2, y: .5, z: 0 });
  });

  it.each(['throw', 'context loss', 'surface change', 'attachment change', 'invalidate'] as const)('discards submitted observations after %s and restores hooks', mode => {
    const f = fixture();
    if (mode === 'throw') f.state.throwAfterDraw = true;
    if (mode === 'context loss') f.state.afterDraw = () => { f.state.lost = true; };
    if (mode === 'surface change') f.state.afterDraw = () => { (f.surface().viewport as { width: number }).width = 180; };
    if (mode === 'attachment change') f.state.afterDraw = () => { f.setFrame({ binding: binding({ assetRevision: 2 }), frameSequence: 1 }); };
    if (mode === 'invalidate') f.state.afterDraw = () => f.probe.invalidate();
    expect(f.run().available).toBe(false);
    expect((f.mesh as THREE.Mesh).onAfterRender).toBe(THREE.Object3D.prototype.onAfterRender);
  });

  it('accepts equivalent fresh surface observations regardless of property insertion order', () => {
    const f = fixture();
    f.state.afterDraw = () => f.setSurface({ clip: { bottom: 100, right: 200, top: 0, left: 0 },
      viewport: { height: 100, width: 200 }, visible: true, connected: true });
    expect(measurement(f.run()).maximumFootprint.width).toBeCloseTo(10);
    f.setFrame({ ...f.frame(), frameSequence: 2 });
    f.state.afterDraw = () => f.setSurface({ ...f.surface(), clip: { ...f.surface().clip, right: 199 } });
    expect(f.run()).toEqual({ available: false, reason: 'invalid_surface' });
  });

  it('consumes failed frame sequences and rejects stale source/layout registration', () => {
    const f = fixture(); const old = f.frame(); f.state.throwAfterDraw = true; expect(f.run().available).toBe(false);
    f.state.throwAfterDraw = false; expect(f.run()).toEqual({ available: false, reason: 'stale_frame' });
    expect(f.probe.bind(binding(), f.root, f.camera, new THREE.Matrix4())).toBe(false);
    const next = { binding: binding({ bindingSequence: 2, sourceWorldId: 'new-lineage', layoutRevision: 2 }), frameSequence: 2 };
    expect(f.probe.bind(next.binding, f.root, f.camera, new THREE.Matrix4())).toBe(true); f.setFrame(next);
    expect(f.probe.renderFrame(old, f.scene, f.port, f.surface, () => next, () => 0)).toEqual({ available: false, reason: 'stale_frame' });
    expect(f.run().available).toBe(true);
  });

  it.each(['perspective', 'shader', 'outline', 'displacement', 'instanced', 'custom hook'] as const)('fails honestly for unsupported %s', mode => {
    let mesh: THREE.Mesh = new THREE.Mesh(triangle(), new THREE.MeshBasicMaterial());
    if (mode === 'shader') mesh.material = new THREE.ShaderMaterial();
    if (mode === 'outline') { const material = new MToonMaterial(); material.isOutline = true; mesh.material = material; }
    if (mode === 'displacement') mesh.material = new THREE.MeshStandardMaterial({ displacementMap: new THREE.Texture() });
    if (mode === 'instanced') mesh = new THREE.InstancedMesh(triangle(), new THREE.MeshBasicMaterial(), 1);
    if (mode === 'custom hook') mesh.onBeforeRender = () => {};
    const f = fixture(mesh);
    if (mode === 'perspective') { const camera = new THREE.PerspectiveCamera(); expect(f.probe.bind(binding({ bindingSequence: 2 }), f.root, camera, new THREE.Matrix4())).toBe(true); f.setFrame({ binding: binding({ bindingSequence: 2 }), frameSequence: 2 }); }
    expect(f.run()).toEqual({ available: false, reason: mode === 'perspective' ? 'unsupported_camera' : 'unsupported_deformation' });
  });

  it('supports current MToon body morph/skin geometry, without promising outline support', () => {
    const material = new MToonMaterial(); material.isOutline = false;
    expect(measurement(fixture(new THREE.Mesh(triangle(), material)).run()).vertices).toBe(3);
  });

  it('supports stock copied MToon bodies with texture, UV and shading options', () => {
    const source = new MToonMaterial(); source.map = new THREE.Texture(); source.normalMap = new THREE.Texture();
    source.outlineWidthMode = 'worldCoordinates'; source.outlineWidthFactor = .4;
    source.v0CompatShade = true; source.uvAnimationScrollXSpeedFactor = .2;
    const material = source.clone(); material.update(.1);
    expect(measurement(fixture(new THREE.Mesh(triangle(), material)).run()).vertices).toBe(3);
  });

  it.each(['basic', 'standard', 'physical', 'toon', 'matcap'] as const)('preserves stock Three %s material defines', kind => {
    const material = kind === 'standard' ? new THREE.MeshStandardMaterial()
      : kind === 'physical' ? new THREE.MeshPhysicalMaterial()
        : kind === 'toon' ? new THREE.MeshToonMaterial()
          : kind === 'matcap' ? new THREE.MeshMatcapMaterial() : new THREE.MeshBasicMaterial();
    expect(measurement(fixture(new THREE.Mesh(triangle(), material)).run()).vertices).toBe(3);
  });

  it.each(['basic macro', 'standard macro', 'changed stock tag', 'inherited macro', 'inherited stock tag'] as const)
  ('rejects ordinary-material %s that bypasses the default compile hook', mode => {
    const material = mode === 'basic macro' ? new THREE.MeshBasicMaterial() : new THREE.MeshStandardMaterial();
    const stock = material instanceof THREE.MeshStandardMaterial ? { STANDARD: '' } : {};
    const defines = mode === 'inherited macro' ? Object.assign(Object.create({ transformed: 'position + vec3(100.0)' }), stock)
      : mode === 'inherited stock tag' ? Object.create({ STANDARD: '' })
        : mode === 'changed stock tag' ? { STANDARD: '\n#define transformed position + vec3(100.0)' }
          : { ...stock, transformed: 'position + vec3(100.0)' };
    Object.assign(material, { defines });
    expect(fixture(new THREE.Mesh(triangle(), material)).run()).toEqual({ available: false, reason: 'unsupported_deformation' });
  });

  it.each(['subclass', 'compile hook', 'render hook', 'vertex shader', 'fragment shader', 'defines generator',
    'outline define', 'custom macro', 'inherited outline', 'program key', 'displacement', 'wireframe', 'boolean spoof'] as const)
  ('rejects MToon compatibility exceptions for customized %s', mode => {
    let material = new MToonMaterial();
    if (mode === 'subclass') { class CustomMToon extends MToonMaterial {} material = new CustomMToon(); }
    if (mode === 'compile hook') material.onBeforeCompile = shader => { shader.vertexShader += '\n// custom vertex path'; };
    if (mode === 'render hook') material.onBeforeRender = () => {};
    if (mode === 'vertex shader') material.vertexShader += '\n// custom vertex path';
    if (mode === 'fragment shader') material.fragmentShader += '\n// custom fragment path';
    if (mode === 'defines generator') Object.defineProperty(material, '_generateDefines', { value: () => ({ OUTLINE: true }) });
    if (mode === 'outline define') material.defines.OUTLINE = true;
    if (mode === 'custom macro') material.defines.transformed = 'position + vec3(100.0)';
    if (mode === 'inherited outline') material.defines = Object.create({ OUTLINE: true });
    if (mode === 'program key') material.customProgramCacheKey = () => 'custom-program';
    if (mode === 'displacement') Object.assign(material, { displacementMap: new THREE.Texture() });
    if (mode === 'wireframe') material.wireframe = true;
    const candidate = mode === 'boolean spoof'
      ? Object.assign(new THREE.ShaderMaterial(), { isMToonMaterial: true, isOutline: false }) : material;
    expect(fixture(new THREE.Mesh(triangle(), candidate)).run()).toEqual({ available: false, reason: 'unsupported_deformation' });
  });

  it.each(['empty', 'nonfinite', 'offscreen', 'depth', 'hidden', 'zero viewport', 'scissor', 'offscreen target'] as const)('rejects %s without a placeholder footprint', mode => {
    const mesh = new THREE.Mesh(mode === 'empty' ? new THREE.BufferGeometry() : triangle(), new THREE.MeshBasicMaterial());
    const f = fixture(mesh);
    if (mode === 'nonfinite') mesh.geometry.attributes.position.setX(0, NaN);
    if (mode === 'offscreen') mesh.position.x = 100;
    if (mode === 'depth') mesh.position.z = 200;
    if (mode === 'hidden') f.setSurface({ ...f.surface(), visible: false });
    if (mode === 'zero viewport') f.setSurface({ ...f.surface(), viewport: { width: 0, height: 100 } });
    if (mode === 'scissor') f.state.scissor = true;
    if (mode === 'offscreen target') f.state.target = true;
    expect(f.run().available).toBe(false);
  });

  it('bounds dense trees, vertex work and measurement CPU clock without traversing unbounded data', () => {
    const tree = new THREE.Group(); for (let i = 0; i < GEOMETRY_MAX_NODES; i++) tree.add(new THREE.Group());
    const dense = fixture(tree); expect(dense.run()).toEqual({ available: false, reason: 'measurement_budget' }); expect(dense.state.renders).toBe(0);
    const geometry = new THREE.BufferGeometry(); geometry.setAttribute('position', new THREE.Float32BufferAttribute(new Float32Array((GEOMETRY_MAX_VERTEX_VISITS + 1) * 3), 3));
    expect(fixture(new THREE.Mesh(geometry, new THREE.MeshBasicMaterial())).run()).toEqual({ available: false, reason: 'measurement_budget' });
    let now = 0; expect(fixture().run(() => now += 4)).toEqual({ available: false, reason: 'measurement_budget' });
  });

  it('rejects excessive draw groups before the renderer can iterate them', () => {
    const geometry = triangle();
    for (let i = 0; i <= GEOMETRY_MAX_DRAW_GROUPS; i++) geometry.addGroup(0, 3, 0);
    const f = fixture(new THREE.Mesh(geometry, [new THREE.MeshBasicMaterial()]));
    expect(f.run()).toEqual({ available: false, reason: 'measurement_budget' });
    expect(f.state.renders).toBe(0);
    expect((f.mesh as THREE.Mesh).onAfterRender).toBe(THREE.Object3D.prototype.onAfterRender);
  });

  it('maps canonical targets without clamping or pretending an unreachable slot was reached', () => {
    const m = measurement(fixture().run()); const safe = { min: { x: 0, y: 0 }, max: { x: 1_000_000, y: 1_000_000 } };
    expect(mapReachableCanonical({ x: 500_000, y: 500_000 }, safe, m, 8)?.target).toEqual({ x: 100, y: 50 });
    expect(mapReachableCanonical({ x: 0, y: 500_000 }, safe, m, 8)).toBeNull();
    expect(mapReachableCanonical({ x: NaN, y: 500_000 }, safe, m, 8)).toBeNull();
    expect(mapReachableCanonical({ x: 500_000, y: 500_000 }, { min: { x: 490_000, y: 490_000 }, max: { x: 510_000, y: 510_000 } }, m, 8)).toBeNull();
    expect(Object.hasOwn(m, 'complete')).toBe(false);
  });

  it.each(['width', 'height'] as const)('rejects a collapsed %s centre interval even for an exactly fitting target', axis => {
    const m = measurement(fixture().run()); const safe = { min: { x: 0, y: 0 }, max: { x: 1_000_000, y: 1_000_000 } };
    const exactFit = { ...m, maximumFootprint: { ...m.maximumFootprint, [axis]: m.viewport[axis] - 16 } };
    expect(mapReachableCanonical({ x: 500_000, y: 500_000 }, safe, exactFit, 8)).toBeNull();
    const corridor = { ...m, maximumFootprint: { ...exactFit.maximumFootprint, [axis]: m.viewport[axis] - 17 } };
    expect(mapReachableCanonical({ x: 500_000, y: 500_000 }, safe, corridor, 8)).not.toBeNull();
  });
});


describe('explicit current CPU-geometry enclosure policy',()=>{
  it('keeps default exact semantics and reports honest zero live leaf evaluations for a prepared conservative range',()=>{
    const exact=measurement(fixture().run());expect(exact.enclosureKind).toBe('exact_submitted_geometry_v1');expect(exact.vertices).toBe(3);
    const f=fixture(new THREE.Mesh(new THREE.SphereGeometry(.5,8,4),new THREE.MeshBasicMaterial()),CONSERVATIVE_ENCLOSURE_KIND);
    expect(f.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});
    f.setFrame({...f.frame(),frameSequence:2});const enclosed=measurement(f.run());
    expect(enclosed.enclosureKind).toBe(CONSERVATIVE_ENCLOSURE_KIND);expect(enclosed.vertices).toBe(0);expect(enclosed.meshes).toBe(1);
    expect(enclosed.posedBounds.right).toBeGreaterThan(enclosed.posedBounds.left);
    const absent=fixture(new THREE.Mesh(new THREE.SphereGeometry(.5,8,4),new THREE.MeshBasicMaterial()),CONSERVATIVE_ENCLOSURE_KIND);
    absent.state.draw=false;expect(absent.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,reason:'empty_geometry'});
  });
  it('rejects a conservative clipped enclosure even when default exact submitted points fit',()=>{
    const points=Array.from({length:120*3},(_,i)=>[-.8,0,0,0,.8,0,0,0,0][i%9]);
    const make=(kind?:typeof CONSERVATIVE_ENCLOSURE_KIND)=>{
      const geometry=new THREE.BufferGeometry().setAttribute('position',new THREE.Float32BufferAttribute(points,3));
      const mesh=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial());mesh.rotation.z=Math.PI/4;const f=fixture(mesh,kind);
      // Keep the supported elevated camera: a front-only camera cannot map
      // both ground axes. Its X projection is unchanged by the elevation.
      f.setSurface({...f.surface(),clip:{left:93,top:0,right:200,bottom:100}});
      f.scene.updateMatrixWorld(true);f.camera.updateMatrixWorld(true);
      const pv=new THREE.Matrix4().multiplyMatrices(f.camera.projectionMatrix,f.camera.matrixWorldInverse),point=new THREE.Vector3();
      let referenceLeft=Infinity;
      // Independent point oracle: every actual submitted CPU point fits.
      for(let i=0;i<geometry.attributes.position.count;i++){
        mesh.getVertexPosition(i,point).applyMatrix4(mesh.matrixWorld).applyMatrix4(pv);
        const x=(point.x+1)*100,y=(1-point.y)*50;referenceLeft=Math.min(referenceLeft,x);
        expect(x).toBeGreaterThan(93);expect(x).toBeLessThanOrEqual(200);expect(y).toBeGreaterThanOrEqual(0);expect(y).toBeLessThanOrEqual(100);
        expect(point.z).toBeGreaterThan(-1);expect(point.z).toBeLessThan(1);
      }
      // The source box also contains this unsubmitted corner; its projected
      // enclosure is intentionally wider than the actual triangle.
      point.set(geometry.attributes.position.getX(0),geometry.attributes.position.getY(1),0).applyMatrix4(mesh.matrixWorld).applyMatrix4(pv);
      expect((point.x+1)*100).toBeLessThan(93);
      return {...f,referenceLeft};
    };
    const actual=make(),exact=measurement(actual.run());expect(exact.posedBounds.left).toBeGreaterThan(93);
    expect(exact.posedBounds.left).toBeCloseTo(actual.referenceLeft,9);
    const conservative=make(CONSERVATIVE_ENCLOSURE_KIND);expect(conservative.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});
    conservative.setFrame({...conservative.frame(),frameSequence:2});expect(conservative.run()).toEqual({available:false,reason:'clipped_geometry'});
  });
  it('unions current exact morph geometry with rigid enclosures under the common conservative claim',()=>{
    const group=new THREE.Group(),rigid=new THREE.Mesh(new THREE.SphereGeometry(.3,8,4),new THREE.MeshBasicMaterial());rigid.position.x=-.5;
    const geometry=triangle();geometry.morphTargetsRelative=true;
    geometry.morphAttributes.position=[new THREE.Float32BufferAttribute([0,0,0,0,0,0,0,1,0],3)];
    const morph=new THREE.Mesh(geometry,new THREE.MeshBasicMaterial());morph.morphTargetInfluences![0]=1;morph.position.x=.5;group.add(rigid,morph);
    const f=fixture(group,CONSERVATIVE_ENCLOSURE_KIND);expect(f.run(()=>0,{allowCoveredPreparation:true})).toEqual({available:false,preparing:true});
    f.setFrame({...f.frame(),frameSequence:2});const m=measurement(f.run());
    expect(m.enclosureKind).toBe(CONSERVATIVE_ENCLOSURE_KIND);expect(m.meshes).toBe(2);expect(m.vertices).toBe(3);
    expect(m.posedBounds.top).toBeLessThan(40);
  });
});
