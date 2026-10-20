// SPDX-License-Identifier: MIT
/** Unwired geometry adapter. Only synchronous successful render callbacks can
 * contribute posed geometry; these observations are never native arrival. */
import * as THREE from 'three';
import type { CssPoint, CssRect, CssSize, GroundBasis, MotionBinding } from './world-motion';

export type GeometryBinding = Readonly<Pick<MotionBinding, 'actorId' | 'sourceWorldId'
  | 'renderSessionId' | 'bindingSequence' | 'worldInstanceId' | 'stageId'
  | 'configRevision' | 'topologyRevision' | 'layoutRevision' | 'assetRevision'
  | 'measurementRevision' | 'scale'>>;
export type GeometryFrame = Readonly<{ binding: GeometryBinding; frameSequence: number }>;
export type SurfaceObservation = Readonly<{
  connected: boolean; visible: boolean; viewport: CssSize; clip: CssRect;
}>;
export type RenderPort = Pick<THREE.WebGLRenderer, 'render' | 'getSize' | 'getViewport'
  | 'getScissorTest' | 'getRenderTarget'> & {
  getContext(): Pick<WebGLRenderingContext, 'isContextLost'>;
};
export type GeometryUnavailable = 'invalid_binding' | 'stale_frame' | 'invalid_surface'
  | 'unsupported_camera' | 'unsupported_deformation' | 'measurement_budget'
  | 'empty_geometry' | 'invalid_geometry' | 'clipped_geometry' | 'render_failed';
export type GeometryMeasurement = Readonly<{
  frame: GeometryFrame; viewport: CssSize; clip: CssRect; position: CssPoint;
  posedBounds: CssRect; maximumFootprint: CssSize; groundBasis: GroundBasis;
  /** Initial measured centre convention, not a cached collision envelope. */
  referenceLocal: Readonly<{ x: number; y: number; z: number }>;
  meshes: number; vertices: number;
}>;
export type GeometryResult = Readonly<{ available: true; measurement: GeometryMeasurement }>
  | Readonly<{ available: false; reason: GeometryUnavailable }>;

export const GEOMETRY_MAX_NODES = 2048;
export const GEOMETRY_MAX_MESHES = 256;
export const GEOMETRY_MAX_DRAW_GROUPS = 1024;
export const GEOMETRY_MAX_VERTEX_VISITS = 200_000;
export const GEOMETRY_MAX_DEFORM_OPERATIONS = 2_000_000;
export const GEOMETRY_MAX_CPU_MS = 8;
const LIMIT = 1_000_000;
const stampKeys = ['actorId', 'sourceWorldId', 'renderSessionId', 'bindingSequence',
  'worldInstanceId', 'stageId', 'configRevision', 'topologyRevision', 'layoutRevision',
  'assetRevision', 'measurementRevision', 'scale'] as const;
const ids = new Set<string>(['actorId', 'sourceWorldId', 'renderSessionId', 'worldInstanceId', 'stageId']);
const counter = (n: number) => Number.isSafeInteger(n) && n > 0;
const finite = (n: number) => Number.isFinite(n) && Math.abs(n) <= LIMIT;
const fail = (reason: GeometryUnavailable): GeometryResult => Object.freeze({ available: false, reason });
function validBinding(value: GeometryBinding): boolean {
  return !!value && stampKeys.every(key => ids.has(key)
    ? typeof value[key] === 'string' && /^[A-Za-z0-9_-]{1,96}$/.test(String(value[key]))
    : key === 'scale' ? typeof value[key] === 'number' && finite(value[key]) && Number(value[key]) > 0
      : typeof value[key] === 'number' && counter(Number(value[key])));
}
function same(a: GeometryBinding, b: GeometryBinding): boolean {
  return !!a && !!b && stampKeys.every(key => a[key] === b[key]);
}
function validSurface(value: SurfaceObservation): boolean {
  if (!value?.connected || !value.visible) return false;
  const { viewport: s, clip: r } = value;
  return !!s && !!r && [s.width, s.height].every(n => finite(n) && n > 0)
    && [r.left, r.top, r.right, r.bottom].every(n => finite(n) && n >= 0)
    && r.left < r.right && r.top < r.bottom && r.right <= s.width && r.bottom <= s.height;
}
function matrixValid(matrix: THREE.Matrix4): boolean {
  return matrix.elements.every(finite) && Math.abs(matrix.determinant()) > 1e-12;
}
function boundedRange(start: number, count: number): boolean {
  return Number.isSafeInteger(start) && start >= 0
    && (count === Infinity || Number.isSafeInteger(count) && count >= 0);
}
function materialSupported(material: THREE.Material): boolean {
  const value = material as THREE.Material & { isMToonMaterial?: boolean; isOutline?: boolean;
    isShaderMaterial?: boolean; isRawShaderMaterial?: boolean; displacementMap?: unknown; wireframe?: boolean };
  return !value.displacementMap && value.wireframe !== true
    && material.onBeforeCompile === THREE.Material.prototype.onBeforeCompile
    && material.onBeforeRender === THREE.Material.prototype.onBeforeRender
    && (!value.isShaderMaterial && !value.isRawShaderMaterial || value.isMToonMaterial === true && value.isOutline === false);
}
function groundBasis(camera: THREE.Camera, view: CssSize, at: CssPoint, frame: THREE.Matrix4): GroundBasis | null {
  const inverse = frame.clone().invert();
  const origin = new THREE.Vector3().applyMatrix4(frame);
  const normal = new THREE.Vector3(0, 1, 0).applyNormalMatrix(new THREE.Matrix3().getNormalMatrix(frame));
  const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(normal, origin);
  // Invert the exact projection/view matrices used for the submitted frame,
  // rather than trusting a possibly stale camera.projectionMatrixInverse cache.
  const unproject = new THREE.Matrix4().multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse).invert();
  const hit = (x: number, y: number) => {
    const near = new THREE.Vector3(x / view.width * 2 - 1, 1 - y / view.height * 2, -1).applyMatrix4(unproject);
    const far = new THREE.Vector3(x / view.width * 2 - 1, 1 - y / view.height * 2, 1).applyMatrix4(unproject);
    const point = new THREE.Ray(near, far.sub(near).normalize()).intersectPlane(plane, new THREE.Vector3());
    return point?.applyMatrix4(inverse);
  };
  const p = hit(at.x, at.y), x = hit(at.x + 1, at.y), y = hit(at.x, at.y + 1);
  if (!p || !x || !y) return null;
  const right = { x: x.x - p.x, z: x.z - p.z }, down = { x: y.x - p.x, z: y.z - p.z };
  if (![right.x, right.z, down.x, down.z].every(n => Number.isFinite(n) && Math.abs(n) <= 10_000)
    || Math.abs(right.x * down.z - right.z * down.x) < 1e-12) return null;
  return Object.freeze({ right: Object.freeze(right), down: Object.freeze(down) });
}

/** One local probe per loaded actor attachment. Renderer, scene and CPU mesh
 * objects come from trusted application code, never JSON/model/tool payloads. */
export class WorldRenderGeometry {
  private binding?: GeometryBinding;
  private root?: THREE.Object3D;
  private camera?: THREE.Camera;
  private ground?: THREE.Matrix4;
  private reference?: THREE.Vector3;
  private highBinding = 0;
  private lastFrame = 0;
  private busy = false;

  bind(binding: GeometryBinding, root: THREE.Object3D, camera: THREE.Camera,
    groundToWorld: THREE.Matrix4, referenceLocal?: THREE.Vector3): boolean {
    if (!validBinding(binding) || binding.bindingSequence <= this.highBinding
      || !root?.isObject3D || !camera?.isCamera || !matrixValid(groundToWorld)
      || groundToWorld.elements[3] !== 0 || groundToWorld.elements[7] !== 0
      || groundToWorld.elements[11] !== 0 || groundToWorld.elements[15] !== 1
      || referenceLocal && !referenceLocal.toArray().every(finite)) return false;
    const previous = this.binding;
    this.highBinding = binding.bindingSequence;
    this.binding = Object.freeze({ ...binding }); this.root = root; this.camera = camera;
    this.ground = groundToWorld.clone(); this.reference = referenceLocal?.clone();
    if (previous?.renderSessionId !== binding.renderSessionId) this.lastFrame = 0;
    return true;
  }
  invalidate(): void { this.binding = undefined; this.root = undefined; this.camera = undefined; this.ground = undefined; this.reference = undefined; }

  /** Installs temporary after-draw hooks, invokes the actual synchronous render,
   * then checks the current attachment/surface before publishing any geometry.
   * A throwing/lost/stale/partial render publishes nothing. No matrices, mixer,
   * morphs or skeleton are refreshed after draw into a different pose. */
  renderFrame(frame: GeometryFrame, scene: THREE.Scene, renderer: RenderPort,
    observeSurface: () => SurfaceObservation, currentFrame: () => GeometryFrame,
    clock: () => number): GeometryResult {
    const binding = this.binding, root = this.root, camera = this.camera, ground = this.ground;
    if (this.busy || !binding || !root || !camera || !ground || !same(frame.binding, binding)
      || !counter(frame.frameSequence) || frame.frameSequence <= this.lastFrame) return fail('stale_frame');
    // Sequence is consumed even by a failed frame; it cannot later be replayed.
    const frameSequence = frame.frameSequence;
    this.lastFrame = frameSequence;
    const current = () => {
      const value = currentFrame();
      return this.binding === binding && value.frameSequence === frameSequence && same(value.binding, binding);
    };
    type Hook = THREE.Object3D['onAfterRender'];
    const hooks: { mesh: THREE.Mesh; original: Hook; installed: Hook }[] = [];
    let reason: GeometryUnavailable | undefined;
    let elapsed = 0, segment = 0, previousClock = -Infinity;
    const read = () => {
      const n = clock();
      if (!Number.isFinite(n) || n < previousClock) throw new Error('measurement_budget');
      previousClock = n; return n;
    };
    const begin = () => { segment = read(); };
    const check = () => { if (elapsed + read() - segment > GEOMETRY_MAX_CPU_MS) throw new Error('measurement_budget'); };
    const end = () => { check(); elapsed += read() - segment; };
    const viewport = new THREE.Vector2(), glViewport = new THREE.Vector4();
    let surface: SurfaceObservation;
    const bounds = new THREE.Box3(), css = { left: Infinity, top: Infinity, right: -Infinity, bottom: -Infinity };
    const seen = new Set<THREE.Mesh>();
    let visits = 0, operations = 0, depthMin = Infinity, depthMax = -Infinity;
    const projected = new THREE.Vector3(), world = new THREE.Vector3();
    const pv = new THREE.Matrix4();
    let renderedMatrices: number[] | undefined;
    const matrices = () => [...root.matrixWorld.elements, ...camera.projectionMatrix.elements, ...camera.matrixWorldInverse.elements];
    const sameMatrices = () => !renderedMatrices || matrices().every((n, i) => n === renderedMatrices![i]);
    const measure = (mesh: THREE.Mesh, geometry: THREE.BufferGeometry, material: THREE.Material,
      group: { start: number; count: number } | null) => {
      if (reason || !material.visible || !material.colorWrite || material.opacity <= 0) return;
      begin();
      try {
        const extra = mesh as THREE.Mesh & { isInstancedMesh?: boolean; isBatchedMesh?: boolean };
        if (extra.isInstancedMesh || extra.isBatchedMesh || geometry instanceof THREE.InstancedBufferGeometry
          || !materialSupported(material)) throw new Error('unsupported_deformation');
        if (!matrixValid(mesh.matrixWorld) || !matrixValid(camera.matrixWorld)
          || !matrixValid(camera.matrixWorldInverse) || !matrixValid(camera.projectionMatrix)) throw new Error('invalid_geometry');
        if (!sameMatrices()) throw new Error('invalid_geometry');
        renderedMatrices ??= matrices();
        const position = geometry.attributes.position;
        if (!position || position.itemSize !== 3 || !counter(position.count)) throw new Error('empty_geometry');
        const morphs = geometry.morphAttributes.position ?? [];
        if (morphs.length > 64 || morphs.some(m => m.count !== position.count || m.itemSize !== 3)
          || mesh.morphTargetInfluences?.some(n => !Number.isFinite(n))) throw new Error('invalid_geometry');
        if (mesh instanceof THREE.SkinnedMesh) {
          const skin = mesh as THREE.SkinnedMesh;
          if (!geometry.attributes.skinIndex || !geometry.attributes.skinWeight || skin.skeleton.bones.length > 512
            || skin.skeleton.bones.length !== skin.skeleton.boneInverses.length
            || skin.skeleton.bones.some((bone, i) => !matrixValid(bone.matrixWorld) || !matrixValid(skin.skeleton.boneInverses[i]))) throw new Error('invalid_geometry');
        }
        if (mesh.getVertexPosition !== (mesh instanceof THREE.SkinnedMesh ? THREE.SkinnedMesh.prototype.getVertexPosition : THREE.Mesh.prototype.getVertexPosition)) throw new Error('unsupported_deformation');
        const range = geometry.drawRange, index = geometry.index;
        if (!boundedRange(range.start, range.count) || group && !boundedRange(group.start, group.count)) throw new Error('invalid_geometry');
        const start = Math.max(range.start, group?.start ?? 0);
        const endAt = Math.min(range.start + range.count, group ? group.start + group.count : Infinity, index?.count ?? position.count);
        if (!Number.isSafeInteger(endAt) || endAt <= start) return;
        if (visits + endAt - start > GEOMETRY_MAX_VERTEX_VISITS) throw new Error('measurement_budget');
        operations += (endAt - start) * (1 + morphs.length + (mesh instanceof THREE.SkinnedMesh ? 4 : 0));
        if (operations > GEOMETRY_MAX_DEFORM_OPERATIONS) throw new Error('measurement_budget');
        pv.multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse);
        for (let i = start; i < endAt; i++) {
          if ((visits++ & 63) === 0) check();
          const vertex = index ? index.getX(i) : i;
          if (!Number.isSafeInteger(vertex) || vertex < 0 || vertex >= position.count) throw new Error('invalid_geometry');
          mesh.getVertexPosition(vertex, world).applyMatrix4(mesh.matrixWorld);
          if (!world.toArray().every(finite)) throw new Error('invalid_geometry');
          bounds.expandByPoint(world); projected.copy(world).applyMatrix4(pv);
          if (!projected.toArray().every(Number.isFinite)) throw new Error('invalid_geometry');
          if (projected.z < -1 || projected.z > 1) throw new Error('clipped_geometry');
          const x = (projected.x + 1) * surface.viewport.width / 2, y = (1 - projected.y) * surface.viewport.height / 2;
          if (x < surface.clip.left || x > surface.clip.right || y < surface.clip.top || y > surface.clip.bottom) throw new Error('clipped_geometry');
          css.left = Math.min(css.left, x); css.right = Math.max(css.right, x);
          css.top = Math.min(css.top, y); css.bottom = Math.max(css.bottom, y);
          depthMin = Math.min(depthMin, projected.z); depthMax = Math.max(depthMax, projected.z);
        }
        seen.add(mesh); end();
      } catch (error) {
        reason = error instanceof Error && ['unsupported_deformation', 'measurement_budget', 'empty_geometry', 'clipped_geometry'].includes(error.message)
          ? error.message as GeometryUnavailable : 'invalid_geometry';
      }
    };
    this.busy = true;
    try {
      if (!current()) return fail('stale_frame');
      if (!(camera instanceof THREE.OrthographicCamera)) return fail('unsupported_camera');
      const before = observeSurface();
      if (!validSurface(before)) return fail('invalid_surface');
      surface = Object.freeze({ connected: before.connected, visible: before.visible, viewport: Object.freeze({ ...before.viewport }), clip: Object.freeze({ ...before.clip }) });
      if (!validSurface(surface) || renderer.getContext().isContextLost() || renderer.getScissorTest() || renderer.getRenderTarget() !== null) return fail('invalid_surface');
      renderer.getSize(viewport); renderer.getViewport(glViewport);
      if (viewport.x !== surface.viewport.width || viewport.y !== surface.viewport.height || glViewport.x !== 0 || glViewport.y !== 0
        || glViewport.z !== viewport.x || glViewport.w !== viewport.y) return fail('invalid_surface');
      if (scene.overrideMaterial || scene.onBeforeRender !== THREE.Object3D.prototype.onBeforeRender
        || scene.onAfterRender !== THREE.Object3D.prototype.onAfterRender) return fail('unsupported_deformation');
      let parent: THREE.Object3D | null = root, parents = 0;
      while (parent && parent !== scene && parents++ < 64) { if (!parent.visible) return fail('empty_geometry'); parent = parent.parent; }
      if (parent !== scene) return fail('invalid_geometry');
      begin();
      const pending: THREE.Object3D[] = [root]; let nodes = 0, drawGroups = 0;
      while (pending.length) {
        if (++nodes > GEOMETRY_MAX_NODES) return fail('measurement_budget');
        const node = pending.pop()!;
        if (!node.visible) continue;
        if (pending.length + node.children.length + nodes > GEOMETRY_MAX_NODES) return fail('measurement_budget');
        pending.push(...node.children);
        if (node instanceof THREE.Mesh) {
          drawGroups += Math.max(1, node.geometry.groups.length);
          if (drawGroups > GEOMETRY_MAX_DRAW_GROUPS) return fail('measurement_budget');
          if (hooks.length >= GEOMETRY_MAX_MESHES || node.onBeforeRender !== THREE.Object3D.prototype.onBeforeRender
            || node.onAfterRender !== THREE.Object3D.prototype.onAfterRender) return fail('unsupported_deformation');
          const original = node.onAfterRender;
          const installed: Hook = function (r, s, c, g, m, group) {
            if (s !== scene || c !== camera) { reason = 'stale_frame'; return; }
            // @types/three names Object3D Group here; runtime passes a geometry range.
            measure(node, g, m, group as unknown as { start: number; count: number } | null);
            original.call(this, r, s, c, g, m, group);
          };
          hooks.push({ mesh: node, original, installed }); node.onAfterRender = installed;
        } else if ((node as THREE.Object3D & { isLine?: boolean; isPoints?: boolean; isSprite?: boolean }).isLine
          || (node as THREE.Points).isPoints || (node as THREE.Sprite).isSprite) return fail('unsupported_deformation');
        check();
      }
      end();
      // The only path that can populate the bounds is the renderer's draw hook.
      renderer.render(scene, camera);
      if (reason) return fail(reason);
      if (!current()) return fail('stale_frame');
      if (!sameMatrices()) return fail('invalid_geometry');
      const after = observeSurface();
      if (!validSurface(after) || JSON.stringify(after) !== JSON.stringify(surface) || renderer.getContext().isContextLost()) return fail('invalid_surface');
      renderer.getSize(viewport); renderer.getViewport(glViewport);
      if (renderer.getScissorTest() || renderer.getRenderTarget() !== null || viewport.x !== surface.viewport.width || viewport.y !== surface.viewport.height
        || glViewport.x !== 0 || glViewport.y !== 0 || glViewport.z !== viewport.x || glViewport.w !== viewport.y) return fail('invalid_surface');
      if (!seen.size || bounds.isEmpty() || css.right <= css.left || css.bottom <= css.top) return fail('empty_geometry');
      begin();
      let reference = this.reference?.clone();
      if (!reference) {
        reference = new THREE.Vector3((css.left + css.right) / surface.viewport.width - 1,
          1 - (css.top + css.bottom) / surface.viewport.height, (depthMin + depthMax) / 2).applyMatrix4(pv.clone().invert());
        if (!matrixValid(root.matrixWorld)) return fail('invalid_geometry');
        reference.applyMatrix4(root.matrixWorld.clone().invert());
      }
      projected.copy(reference).applyMatrix4(root.matrixWorld).applyMatrix4(pv);
      const position = { x: (projected.x + 1) * surface.viewport.width / 2, y: (1 - projected.y) * surface.viewport.height / 2 };
      if (!reference.toArray().every(finite) || !projected.toArray().every(Number.isFinite)
        || position.x < css.left || position.x > css.right || position.y < css.top || position.y > css.bottom) return fail('invalid_geometry');
      const footprint = { width: 2 * Math.max(position.x - css.left, css.right - position.x), height: 2 * Math.max(position.y - css.top, css.bottom - position.y) };
      const basis = groundBasis(camera, surface.viewport, position, ground);
      if (!basis || ![footprint.width, footprint.height].every(n => finite(n) && n > 0)) return fail('invalid_geometry');
      end();
      if (!current()) return fail('stale_frame');
      this.reference = reference;
      const measurement: GeometryMeasurement = Object.freeze({ frame: Object.freeze({ binding, frameSequence }),
        viewport: Object.freeze({ ...surface.viewport }), clip: Object.freeze({ ...surface.clip }), position: Object.freeze(position),
        posedBounds: Object.freeze({ ...css }), maximumFootprint: Object.freeze(footprint), groundBasis: basis,
        referenceLocal: Object.freeze({ x: reference.x, y: reference.y, z: reference.z }), meshes: seen.size, vertices: visits });
      return Object.freeze({ available: true, measurement });
    } catch (error) {
      return fail(error instanceof Error && error.message === 'measurement_budget' ? 'measurement_budget' : 'render_failed');
    } finally {
      for (const { mesh, original, installed } of hooks) if (mesh.onAfterRender === installed) mesh.onAfterRender = original;
      this.busy = false;
    }
  }
}

/** Sanity check only. Native must independently authorize/match this exact
 * mapping/footprint/slot; the renderer cannot grant a reachable target. */
export function mapReachableCanonical(target: CssPoint, safe: Readonly<{ min: CssPoint; max: CssPoint }>,
  measurement: GeometryMeasurement, padding: number): Readonly<{ target: CssPoint; safeCenters: CssRect }> | null {
  const canonical = (p: CssPoint) => !!p && [p.x, p.y].every(n => Number.isSafeInteger(n) && n >= 0 && n <= LIMIT);
  if (!canonical(target) || !canonical(safe?.min) || !canonical(safe?.max) || safe.min.x >= safe.max.x || safe.min.y >= safe.max.y
    || !Number.isFinite(padding) || padding < 0 || padding > 256) return null;
  const { viewport: view, maximumFootprint: size, clip } = measurement;
  if (!validSurface({ connected: true, visible: true, viewport: view, clip }) || !size
    || ![size.width, size.height].every(n => finite(n) && n > 0)) return null;
  const safeCenters = { left: Math.max(safe.min.x / LIMIT * view.width, clip.left) + size.width / 2 + padding,
    right: Math.min(safe.max.x / LIMIT * view.width, clip.right) - size.width / 2 - padding,
    top: Math.max(safe.min.y / LIMIT * view.height, clip.top) + size.height / 2 + padding,
    bottom: Math.min(safe.max.y / LIMIT * view.height, clip.bottom) - size.height / 2 - padding };
  const point = { x: target.x / LIMIT * view.width, y: target.y / LIMIT * view.height };
  if (![safeCenters.left, safeCenters.right, safeCenters.top, safeCenters.bottom, point.x, point.y].every(finite)
    || safeCenters.left > safeCenters.right || safeCenters.top > safeCenters.bottom
    || point.x < safeCenters.left || point.x > safeCenters.right || point.y < safeCenters.top || point.y > safeCenters.bottom) return null;
  return Object.freeze({ target: Object.freeze(point), safeCenters: Object.freeze(safeCenters) });
}
