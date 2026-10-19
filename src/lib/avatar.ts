import * as THREE from 'three';
import { DesktopMotionController } from './desktop-motion';
import { AvatarPresentationGate, presentationAbortError, type PresentationRelease } from './avatar-presentation';
import { desktopFraming, measureDesktopModel } from './desktop-framing';
import type { DesktopLayout } from './output-presentation';
export type DesktopAnchor={x:number;y:number;width:number;height:number;visible:boolean;viewport:{width:number;height:number}};
import { availableActions, clipHasTarget, avatarBinding, currentEmotePayload, resolveAvailableAction, unavailableEmote, type AvailableAction } from './emote-capabilities';
import type { EmotePayload } from './presentation';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { createAvatarLoader } from './avatar-loader';
import { disposeAvatarObject as disposeObject } from './avatar-resources';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';
import { VRMExpressionMorphTargetBind, VRMLookAtBoneApplier, VRMLookAtExpressionApplier, VRMUtils, type VRM, type VRMExpression } from '@pixiv/three-vrm';
import { configureBundledAccessories, discoverGenericHumanoid, humanoidArmRest, humanoidFrame, humanoidRotation, humanoidWaveRaise, poseGenericArm, rawHumanoidRig, rotateGenericBone, type GenericHumanoidRig } from './avatar-rig';
import { AuthoredAnimationProperties, finishVrmAnimationFrame, selectEmoteAnimation, selectIdleAnimation } from './avatar-animation';
import { ensureMotionLookAtProxy, isMotionAction, loadMotionClips, transferMotionPose, type MotionAsset } from './avatar-motion';
import type { VRMLookAtQuaternionProxy } from '@pixiv/three-vrm-animation';
import { ambientGestureActions, discoverWalkingLegs, followAmbientPosition, motionLaneHalfWidth, sampleAmbientMotion, sampleWalkingGait, selectWalkingAnimation, type AutopilotVisualConfig, type WalkingLegs } from './autopilot-motion';

export interface AvatarOptions {
  profile: any;
  speaking: boolean;
  level: number;
  emotion: string;
  transparent: boolean;
  autopilot?: AutopilotVisualConfig;
  desktop?:DesktopLayout;
  onDesktopAnchor?:(anchor:DesktopAnchor|undefined)=>void;
}

export interface AvatarModelInfo {
  status: 'loading' | 'ready' | 'error';
  format?: 'builtin' | 'glb' | 'vrm0' | 'vrm1';
  animations: { name: string; duration: number }[];
  proceduralGestures: string[];
  actions?: AvailableAction[];
  avatarBinding?: unknown;
  error?: string;
}

type Robot = {
  root: THREE.Group;
  head: THREE.Group;
  torso: THREE.Group;
  arms: THREE.Group[];
  legs: THREE.Group[];
  eyes: THREE.Group[];
  mouth: THREE.Group;
  jaw: THREE.Mesh;
  antenna: THREE.Mesh;
  accents: THREE.MeshStandardMaterial[];
};

type ActiveEmote = {
  action: string;
  start: number;
  duration: number;
  intensity: number;
  clip?: THREE.AnimationAction;
  fading?: boolean;
};

type Vowel = 'aa' | 'ih' | 'ou' | 'ee' | 'oh';
const VOWELS: readonly Vowel[] = ['aa', 'ih', 'ou', 'ee', 'oh'];
const VOWEL_ALIASES: Record<Vowel, readonly string[]> = {
  aa: ['aa', 'a'], ih: ['ih', 'i'], ou: ['ou', 'u'], ee: ['ee', 'e'], oh: ['oh', 'o'],
};
const VOWEL_SEQUENCE: readonly Vowel[] = ['aa', 'ee', 'aa', 'oh', 'ih', 'aa', 'ou'];
const expressionKey = (name: string) => name.toLowerCase().replace(/[^a-z0-9]/g, '');

function morphVowel(name: string): Vowel | undefined {
  const key = expressionKey(name).replace(/^(?:vrcv|viseme|mouth)/, '');
  return VOWELS.find((vowel) => VOWEL_ALIASES[vowel].includes(key));
}

/** A self-contained renderer shared by the dashboard, native overlay and OBS. */
export class AvatarStage {
  private renderer: THREE.WebGLRenderer;
  private assetLoader: ReturnType<typeof createAvatarLoader>;
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(32, 1, 0.05, 100);
  private desktopCamera=new THREE.OrthographicCamera(-1,1,1,-1,.05,100);
  private get activeCamera(){return this.desktopActive?this.desktopCamera:this.camera;}
  private controls: OrbitControls;
  private clock = new THREE.Clock();
  private robot: Robot;
  private ground = new THREE.Group();
  private travel = new THREE.Group();
  private imported = new THREE.Group();
  private vrm?: VRM;
  private mixer?: THREE.AnimationMixer;
  private clips: THREE.AnimationClip[] = [];
  private authoredProperties?: AuthoredAnimationProperties;
  private idleAction?: THREE.AnimationAction;
  private emote?: ActiveEmote;
  private emoteSource?: 'manual' | 'ambient';
  private emoteNonce: string | number | undefined;
  private humanoidPose = new Map<string, { node: THREE.Object3D; quaternion: THREE.Quaternion }>();
  private humanoidBasis = new THREE.Quaternion();
  private faceMeshes: THREE.Mesh[] = [];
  private faceVowels = new Map<THREE.Mesh, Map<number, Vowel>>();
  private genericMouthRig = false;
  private vrmExpressions = new Map<string, VRMExpression>();
  private mouthExpressions = new Map<Vowel, VRMExpression>();
  private morphMotion = new WeakMap<THREE.BufferGeometry, Map<number, boolean>>();
  private genericHead?: THREE.Object3D;
  private genericChest?: THREE.Object3D;
  private armRig?: GenericHumanoidRig;
  private genericBoneRest = new Map<THREE.Object3D, THREE.Quaternion>();
  private pendingImportedHead?: [number, number, number];
  private walkingLegs?: WalkingLegs;
  private walkingClip?: THREE.AnimationClip;
  private walkingAction?: THREE.AnimationAction;
  private walking = false;
  private walkingPhase = 0;
  private walkAnchor?: THREE.Object3D;
  private walkAnchorRest = new THREE.Vector3();
  private modelForwardYaw = 0;
  private contentWidth = 2.05;
  private contentDepth = 1.35;
  private travelFraction = 0;
  private desktopMotion=new DesktopMotionController();
  private desktopPresentationReady = false;
  private presentation = new AvatarPresentationGate();
  private presentationContext = '';
  private assetState: 'ready' | 'loading' | 'error' = 'ready';
  private desktopFigure={width:1,height:1};
  private desktopModelBounds={center:new THREE.Vector3(0,1.45,0),size:new THREE.Vector3(2.05,2.9,1.35)};
  private lastDesktopAnchor=0;
  private get desktopActive(){return Boolean(this.options.transparent&&this.options.desktop?.active);}
  private ambientSession = '';
  private ambientGesture = '';
  private ambientActions: string[] = ['wave', 'nod', 'celebrate'];
  private walkStrength = 0;
  private contactShadow?: THREE.Mesh;
  private jawBone?: THREE.Object3D;
  private previousBoneOffsets = new Map<THREE.Object3D, THREE.Quaternion>();
  private contentHeight = 2.9;
  private lookAt = new THREE.Object3D();
  private observer: ResizeObserver;
  private motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  private options: AvatarOptions;
  private request = 0;
  private frame = 0;
  private lastRenderTime = 0;
  private disposed = false;
  private asset = '';
  private assetKey = '';
  private emoteReady = false;
  private loadedEmoteBinding = '';
  private motionAbort?: AbortController;
  private motionLookAt?: VRMLookAtQuaternionProxy;
  private cameraMode = '';
  private time = 0;
  private mouthLevel = 0;
  private speechPhase = 0;
  private vowelWeights: Record<Vowel, number> = { aa: 0, ih: 0, ou: 0, ee: 0, oh: 0 };
  private blinkAt = 2.4;

  constructor(private host: HTMLElement, options: AvatarOptions, private onEmoteError?: (message: string) => void, initialAssetUrl?: string) {
    this.options = options;
    this.assetState = initialAssetUrl ? 'loading' : 'ready';
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'high-performance' });
    this.assetLoader = createAvatarLoader(this.renderer);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.75));
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.35;
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.renderer.domElement.setAttribute('aria-hidden', 'true');
    this.renderer.domElement.style.cssText = 'display:block;width:100%;height:100%;touch-action:none';
    this.host.appendChild(this.renderer.domElement);

    this.scene.add(new THREE.HemisphereLight(0xf4f6ff, 0x9695bc, 2.5));
    const key = new THREE.DirectionalLight(0xfff6e8, 3.7);
    key.position.set(-3.5, 6, 5);
    key.castShadow = true;
    key.shadow.mapSize.set(1024, 1024);
    key.shadow.camera.left = -3;
    key.shadow.camera.right = 3;
    key.shadow.camera.top = 5;
    key.shadow.camera.bottom = -2;
    key.shadow.normalBias = 0.025;
    key.shadow.bias = -0.0001;
    this.scene.add(key);
    const rim = new THREE.DirectionalLight(0xaca4ff, 2.8);
    rim.position.set(3, 3, -3);
    this.scene.add(rim);
    const fill = new THREE.DirectionalLight(0x91d9ff, 0.8);
    fill.position.set(4, 1, 4);
    this.scene.add(fill);

    this.robot = createRobot();
    this.desktopModelBounds=measureDesktopModel(this.robot.root);
    this.travel.add(this.robot.root, this.imported);
    this.scene.add(this.travel, this.ground, this.lookAt);
    this.makeGround();
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.08;
    this.controls.enablePan = false;
    this.controls.minDistance = 1.6;
    this.controls.maxDistance = 10;
    this.controls.minPolarAngle = 0.35;
    this.controls.maxPolarAngle = Math.PI / 2 + 0.08;
    this.controls.rotateSpeed = 0.5;
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(host);
    this.update(options);
    this.resize();
    this.animate();
  }

  update(options: AvatarOptions) {
    const context = JSON.stringify([options.profile?.id, options.profile?.avatar?.assetPath, options.profile?.motions ?? [], options.autopilot?.active === true, options.autopilot?.seed, options.autopilot?.startedAt, options.desktop?.active === true]);
    if (this.presentationContext && context !== this.presentationContext) this.presentation?.invalidate();
    this.presentationContext = context;
    this.options = options;
    const avatar = options.profile?.avatar ?? {};
    this.ground.visible = !options.transparent;
    let scale = THREE.MathUtils.clamp(Number(avatar.scale) || 1, 0.3, 2.5);
    if(this.desktopActive){const frame=this.desktopFrame(scale);scale=frame.scale;this.desktopCamera.left=-frame.worldWidth/2;this.desktopCamera.right=frame.worldWidth/2;this.desktopCamera.top=frame.worldHeight/2;this.desktopCamera.bottom=-frame.worldHeight/2;this.desktopCamera.position.set(0,0,20);this.desktopCamera.lookAt(0,0,0);this.desktopCamera.updateProjectionMatrix();this.desktopCamera.updateMatrixWorld();}
    this.robot.root.scale.setScalar(scale);
    this.imported.scale.setScalar(scale);
    const accent = new THREE.Color();
    try { accent.set(avatar.accent || '#80b8e8'); } catch { accent.set('#80b8e8'); }
    for (const material of this.robot.accents) {
      material.color.copy(accent);
      if (material.emissiveIntensity > 0) material.emissive.copy(accent);
    }
    const mode = avatar.camera === 'full' ? 'full' : 'portrait';
    if (mode !== this.cameraMode) {
      this.cameraMode = mode;
      const full = mode === 'full';
      const focusY = this.contentHeight * (full ? 0.465 : 0.586);
      this.camera.position.set(full ? 0.55 : 0.35, focusY + (full ? 0.7 : 0.48), full ? 7.6 : 5.4);
      this.controls.target.set(0, focusY, 0);
      this.controls.update();
    }
    this.controls.enabled = !options.transparent;
    const emote = avatar.emote;
    if (!emote && this.emoteSource === 'manual') this.finishEmote();
    if (!emote && this.emoteNonce !== undefined) {
      this.emoteNonce = undefined;
      this.finishEmote();
    }
    if (emote && emote.nonce !== undefined && emote.nonce !== this.emoteNonce) {
      this.emoteNonce = emote.nonce;
      if(emote.id&&!this.canPresentEmote(emote,options.profile)){this.onEmoteError?.(unavailableEmote);return;}
      this.triggerEmote(String(emote.action || ''), Number(emote.duration), Number(emote.intensity));
    }
  }

  async loadAsset(url?: string, motions: readonly MotionAsset[] = [], outputUrl = '', botId = ''): Promise<void> {
    const next = url || '';
    const emoteBinding = JSON.stringify(avatarBinding(this.options.profile));
    const key = JSON.stringify([next,botId,motions,outputUrl,emoteBinding]);
    if (key === this.assetKey) return;
    this.assetKey = key;
    this.emoteReady = false;
    this.asset = next;
    this.presentation.reset();
    this.assetState = next ? 'loading' : 'ready';
    this.desktopPresentationReady = false;
    const request = ++this.request;
    this.clearImported();
    const abort = this.motionAbort = new AbortController();
    this.robot.root.visible = true;
    this.cameraMode = '';
    this.update(this.options);
    if (!next) { this.emoteReady = !this.options.profile?.avatar?.assetPath || this.options.profile.avatar.assetPath === 'nova'; this.loadedEmoteBinding = emoteBinding; return; }
    try {
      // GLTFLoader can swallow a failed image request and finish with an
      // untextured material. Treat missing resources as an import error.
      const gltf = await this.assetLoader.loadAsync(next);
      if (this.disposed || request !== this.request) {
        disposeObject(gltf.scene);
        return;
      }
      if (gltf.userData.orbitpalResourceFailed) {
        disposeObject(gltf.scene);
        throw new Error('Texturen oder Modelldaten konnten nicht vollständig geladen werden.');
      }
      const vrm = gltf.userData.vrm as VRM | undefined;
      if (vrm) {
        VRMUtils.removeUnnecessaryVertices(gltf.scene);
        VRMUtils.combineSkeletons(gltf.scene);
        VRMUtils.rotateVRM0(vrm);
        vrm.lookAt && (vrm.lookAt.target = this.lookAt);
        this.vrm = vrm;
        this.armRig = rawHumanoidRig(gltf.scene, vrm.humanoid);
        this.humanoidBasis = humanoidFrame(vrm.humanoid);
        for (const name of ['head', 'neck', 'spine', 'chest', 'hips', 'jaw', 'leftUpperArm', 'rightUpperArm', 'leftLowerArm', 'rightLowerArm', 'leftHand', 'rightHand', 'leftUpperLeg', 'rightUpperLeg', 'leftLowerLeg', 'rightLowerLeg', 'leftFoot', 'rightFoot'] as const) {
          const node = vrm.humanoid.getNormalizedBoneNode(name);
          if (node) this.humanoidPose.set(name, { node, quaternion: node.quaternion.clone() });
        }
        this.applyHumanoidRestPose();
        vrm.humanoid.update();
      }
      if (!vrm) {
        this.armRig = discoverGenericHumanoid(gltf.scene);
        if (this.armRig) this.walkingLegs = discoverWalkingLegs(gltf.scene);
        if (this.armRig && !selectIdleAnimation(gltf.animations)) {
          for (const side of ['left', 'right'] as const) poseGenericArm(this.armRig, side, -1.15, -1.15, 1);
        }
      }
      const pathname = new URL(next, window.location.href).pathname;
      configureBundledAccessories(gltf.scene, pathname);
      const normalization = new THREE.Group();
      normalization.add(gltf.scene);
      if (pathname.endsWith('/characters/Fox.glb')) normalization.rotation.y = Math.PI * 0.23;
      normalization.updateMatrixWorld(true);
      const box = new THREE.Box3().setFromObject(normalization);
      const size = box.getSize(new THREE.Vector3());
      if (!Number.isFinite(size.y) || size.y < 0.001) {
        disposeObject(gltf.scene);
        throw new Error('Die Datei enthält kein sichtbares 3D-Modell.');
      }
      const center = box.getCenter(new THREE.Vector3());
      // A quadruped needs a length limit as well as a height limit. Uniform
      // scaling preserves its proportions and keeps the complete body in view.
      const fit = Math.min(2.9 / size.y, 3.1 / Math.max(size.x, 0.001), 3.1 / Math.max(size.z, 0.001));
      this.contentHeight = size.y * fit;
      this.contentWidth = size.x * fit;
      this.contentDepth = size.z * fit;
      normalization.scale.setScalar(fit);
      normalization.position.set(-center.x * fit, -box.min.y * fit, -center.z * fit);
      gltf.scene.traverse((object) => {
        if (object instanceof THREE.Mesh) {
          object.castShadow = true;
          object.receiveShadow = true;
          if (!vrm && object.morphTargetDictionary && object.morphTargetInfluences) {
            this.faceMeshes.push(object);
            const vowels = new Map<number, Vowel>();
            for (const [name, index] of Object.entries(object.morphTargetDictionary)) {
              const vowel = morphVowel(name);
              if (vowel && this.hasMorphMotion(object, index)) vowels.set(index, vowel);
              if (/^(mouthopen|jawopen)$/.test(expressionKey(name)) && this.hasMorphMotion(object, index)) this.genericMouthRig = true;
            }
            if (vowels.size) this.genericMouthRig = true;
            this.faceVowels.set(object, vowels);
          }
        }
        if (!vrm && object instanceof THREE.Bone) {
          const name = object.name.split(':').pop() || object.name;
          if (/^(?:b_)?Head(?:[_.\d].*)?$/i.test(name) && !this.genericHead) this.genericHead = object;
          if (/^(?:b_)?(?:chest|spine(?:0?[123])?|torso)(?:[_.\d].*)?$/i.test(name)) this.genericChest = object;
          if (/(?:^|[_. -])(?:jaw|lowerjaw|mandible)(?:$|[_.\d])/i.test(name) && !this.jawBone) this.jawBone = object;
        }
        if (object instanceof THREE.Bone && /^(?:b_)?(?:hips?|pelvis)(?:[_.\d].*)?$/i.test(object.name.split(':').pop()!.replace(/^mixamorig/i, '')) && !this.walkAnchor) this.walkAnchor = object;
      });
      for (const node of [this.genericHead, this.genericChest, this.jawBone]) if (node) this.genericBoneRest.set(node, node.quaternion.clone());
      this.imported.add(normalization);
      normalization.updateWorldMatrix(true, true);
      if (vrm) this.walkAnchor = vrm.humanoid.getRawBoneNode('hips') ?? this.walkAnchor;
      if (this.walkAnchor) this.walkAnchorRest.copy(this.imported.worldToLocal(this.walkAnchor.getWorldPosition(new THREE.Vector3())));
      const forward = new THREE.Vector3(0, 0, 1);
      if (this.armRig) forward.applyQuaternion(this.armRig.root.getWorldQuaternion(new THREE.Quaternion()).multiply(this.armRig.frame));
      else if (this.genericHead && this.walkAnchor) {
        const direction = this.genericHead.getWorldPosition(new THREE.Vector3()).sub(this.walkAnchor.getWorldPosition(new THREE.Vector3()));
        if (Math.hypot(direction.x, direction.z) > Math.abs(direction.y)) forward.copy(direction);
      }
      this.modelForwardYaw = Math.atan2(forward.x, forward.z);
      this.vrm = vrm;
      if (vrm) {
        this.jawBone = vrm.humanoid.getNormalizedBoneNode('jaw') ?? undefined;
        for (const expression of Object.values(vrm.expressionManager?.expressionMap ?? {})) {
          this.vrmExpressions.set(expressionKey(expression.expressionName), expression);
        }
        // The loader maps VRM 0 a/i/u/e/o to VRM 1 aa/ih/ou/ee/oh.
        // Custom expressions can retain their old names, so resolve both.
        // A preset label alone is insufficient: empty placeholder morphs do
        // not constitute a working mouth rig.
        for (const vowel of VOWELS) {
          for (const alias of VOWEL_ALIASES[vowel]) {
            const expression = this.vrmExpressions.get(alias);
            if (expression?.binds.some((bind) => bind instanceof VRMExpressionMorphTargetBind
              && Math.abs(bind.weight) > 0.00001
              && bind.primitives.some((mesh) => this.hasMorphMotion(mesh, bind.index)))) {
              this.mouthExpressions.set(vowel, expression);
              break;
            }
          }
        }
      }
      this.clips = gltf.animations;
      if (vrm && motions.length) {
        this.motionLookAt = ensureMotionLookAtProxy(vrm);
        try { this.clips = [...gltf.animations,...await loadMotionClips(vrm,motions,outputUrl,botId,abort.signal)]; }
        catch (error) { if (abort.signal.aborted) return; this.onEmoteError?.(error instanceof Error ? error.message : String(error)); }
        if (this.disposed || request !== this.request) return;
      }
      if (this.clips.length) {
        this.mixer = new THREE.AnimationMixer(gltf.scene);
        this.authoredProperties = new AuthoredAnimationProperties(gltf.scene);
        // Prefer the neutral action over e.g. 2H_Melee_Idle or Sit_Chair_Idle
        // in packs containing many activity-specific idle animations.
        const clip = selectIdleAnimation(gltf.animations);
        if (clip) this.idleAction = this.mixer.clipAction(clip).play();
        const walking = selectWalkingAnimation(gltf.animations);
        if (walking) {
          // A separate action prevents an explicit Walking preview from
          // sharing mutable loop/time state with the ambient layer.
          this.walkingClip = walking.clone();
          this.walkingAction = this.mixer.clipAction(this.walkingClip);
        }
      }
      this.desktopModelBounds=measureDesktopModel(this.imported);
      this.emoteReady = true;
      this.loadedEmoteBinding = emoteBinding;
      this.ambientActions = ambientGestureActions(this.clips.filter(clip=>this.getModelInfo().actions?.some(action=>action.action===clip.name)), this.getModelInfo().proceduralGestures);
      this.robot.root.visible = false;
      this.assetState = 'ready';
      this.cameraMode = '';
      this.update(this.options);
    } catch (error) {
      if (request !== this.request || this.disposed) return;
      this.emoteReady = false;
      this.robot.root.visible = true;
      this.assetState = 'error';
      this.presentation.fail(error);
      throw error;
    }
  }

  /** Hold wandering from the request through actual output completion. */
  preparePresentation(signal: AbortSignal): Promise<PresentationRelease> {
    if (signal.aborted || this.disposed) return Promise.reject(presentationAbortError());
    if (this.emoteSource === 'ambient') this.finishEmote();
    return this.presentation.acquire(signal);
  }

  get clipNames(): string[] { return this.clips.map((clip) => clip.name); }

  private proceduralGestures(): string[] {
    if (!this.asset) return ['wave','celebrate','nod','dance','hide'];
    // Generic GLB names do not prove the native rig hierarchy/geometry. Authored
    // clips remain available; procedural gestures require real VRM bones.
    if (!this.vrm) return ['hide'];
    const has=(names:string[])=>names.every(name=>this.humanoidPose.has(name));
    const arms=has(['leftUpperArm','leftLowerArm','rightUpperArm','rightLowerArm']);
    return ['hide',...(arms?['wave','celebrate']:[]),...(has(['head'])?['nod']:[]),...(arms&&has(['spine'])?['dance']:[])];
  }
  private hasLookAtTarget():boolean {
    const vrm=this.vrm;
    if(!vrm?.lookAt||!vrm.humanoid.getNormalizedBoneNode('head'))return false;
    if(vrm.lookAt.applier instanceof VRMLookAtBoneApplier)return Boolean(vrm.humanoid.getRawBoneNode('leftEye')||vrm.humanoid.getRawBoneNode('rightEye'));
    if(vrm.lookAt.applier instanceof VRMLookAtExpressionApplier)return ['lookUp','lookDown','lookLeft','lookRight'].some(name=>Boolean(vrm.expressionManager?.getExpression(name)?.binds.length));
    return false;
  }
  private effectiveClip(clip:THREE.AnimationClip):boolean {
    return clipHasTarget(this.imported,clip,node=>node!==this.motionLookAt||this.hasLookAtTarget());
  }
  getModelInfo(): AvatarModelInfo {
    const actions=availableActions(this.clips,this.proceduralGestures()).filter(action=>action.kind==='gesture'||this.clips.some(clip=>clip.name===action.action&&this.effectiveClip(clip)));
    return {
      status: this.emoteReady ? 'ready' : 'loading', avatarBinding: this.loadedEmoteBinding ? JSON.parse(this.loadedEmoteBinding) : undefined,
      format: !this.asset ? 'builtin' : this.vrm ? this.vrm.meta.metaVersion === '0' ? 'vrm0' : 'vrm1' : 'glb',
      animations: actions.filter(action=>action.kind==='clip').map(action=>({name:action.action,duration:action.duration!})),
      proceduralGestures: actions.filter(action=>action.kind==='gesture').map(action=>action.action), actions,
    };
  }
  canPresentEmote(event:EmotePayload,profile:any=this.options.profile):boolean {
    if(this.disposed||!this.emoteReady||JSON.stringify(avatarBinding(profile))!==this.loadedEmoteBinding||!currentEmotePayload(profile,event))return false;
    const action=resolveAvailableAction(this.getModelInfo().actions??[],event.action);
    if(!action)return false;
    const configured=(profile.emotes??[]).find((emote:any)=>emote.id===event.id);
    if(!configured||resolveAvailableAction(this.getModelInfo().actions??[],configured.action)?.action!==event.action)return false;
    if(action.kind==='gesture')return true;
    let clip:THREE.AnimationClip|undefined;try{clip=selectEmoteAnimation(this.clips,action.action);}catch{return false;}
    if(!clip||!this.mixer)return false;
    // Expressions require actual binds; an empty placeholder cannot speak for
    // a valid facial motion. Other tracks must address in-range scene properties.
    if(clip.tracks.every(track=>track.name.endsWith('.weight'))&&this.vrm){
      const expressions=Object.values(this.vrm.expressionManager?.expressionMap??{});
      return clip.tracks.some(track=>expressions.some(expression=>track.name===`${expression.name}.weight`&&expression.binds.length>0))&&this.effectiveClip(clip);
    }
    return this.effectiveClip(clip);
  }
  presentEmote(event:EmotePayload,profile:any=this.options.profile):boolean {
    if(!this.canPresentEmote(event,profile))return false;
    const nonce=event.nonce??crypto.randomUUID();
    if(!this.triggerEmote(event.action,(event.duration??2.5)*1000,event.intensity??1))return false;
    this.emoteNonce=nonce;return true;
  }

  private get activeAnimationActions(): (THREE.AnimationAction | undefined)[] { return [this.idleAction, this.walkingAction, this.emote?.clip]; }

  private resumeIdleAnimation(fadeSeconds = 0) {
    const idle = this.idleAction;
    if (!idle) return;
    // A completed fadeOut disables the action. play()/fadeIn() alone do
    // not re-enable it; preserve its phase and the operator's pause policy.
    idle.enabled = true;
    idle.setEffectiveWeight(1);
    if (fadeSeconds > 0) idle.fadeIn(fadeSeconds);
    idle.play();
  }

  private setWalking(walking: boolean, immediate = false) {
    if (this.walking === walking && !immediate) return;
    const previous = this.walking;
    this.walking = walking;
    const action = this.walkingAction;
    if (!action) return;
    if (walking) {
      this.idleAction?.fadeOut(0.2);
      action.reset().setLoop(THREE.LoopRepeat, Infinity).setEffectiveWeight(1).setEffectiveTimeScale(0).fadeIn(0.2).play();
    } else {
      const participated = previous || (action.enabled && action.isScheduled() && action.getEffectiveWeight() > 0.0001);
      if (immediate) action.stop(); else action.fadeOut(0.2);
      if (participated && this.idleAction && !this.emote?.clip) {
        this.resumeIdleAnimation(immediate ? 0 : 0.2);
      }
    }
  }

  private canWalk(): boolean {
    if (!this.asset || this.walkingClip) return true;
    if (this.vrm) return ['leftUpperLeg', 'leftLowerLeg', 'leftFoot', 'rightUpperLeg', 'rightLowerLeg', 'rightFoot'].every((name) => this.humanoidPose.has(name));
    return Boolean(this.armRig && this.walkingLegs);
  }

  private desktopFrame(scale:number){return desktopFraming({width:this.host.clientWidth,height:this.host.clientHeight,pixelRatio:window.devicePixelRatio||1,referencePhysicalHeight:this.options.desktop?.referencePhysicalHeight??this.host.clientHeight,scale,contentWidth:this.desktopModelBounds.size.x,contentHeight:this.desktopModelBounds.size.y,contentDepth:this.desktopModelBounds.size.z,fov:this.camera.fov,normalDistance:this.options.profile?.avatar?.camera==='full'?Math.hypot(.55,.7,7.6):Math.hypot(.35,.48,5.4)});}

  private updateAutopilot(delta: number, reduced: boolean) {
    const config = this.options.autopilot;
    const frame = sampleAmbientMotion(config, Date.now());
    const session = frame.active && config ? `${config.startedAt}:${config.seed.slice(0, 128)}` : '';
    if (session !== this.ambientSession) {
      if (this.emoteSource === 'ambient') this.finishEmote();
      this.setWalking(false, true);
      this.ambientSession = session; this.ambientGesture = ''; this.travelFraction = 0;
    }
    const presenting = this.presentation?.requested ?? false;
    const blocked = this.options.speaking || this.emoteSource === 'manual' || Boolean(this.asset && this.robot.root.visible);
    const ambientBlocked = blocked || presenting;
    if ((!frame.active || reduced)&&!this.desktopActive) {
      if (this.emoteSource === 'ambient') this.finishEmote();
      this.setWalking(false, true); this.walkStrength = 0; this.travelFraction = 0;
      this.travel.position.set(0, 0, 0); this.travel.rotation.set(0, 0, 0);
      if (this.contactShadow) { this.contactShadow.position.x = 0; this.contactShadow.position.z = 0; }
      return;
    }
    if (ambientBlocked || !frame.gesture || frame.gesture.key !== this.ambientGesture) {
      if (this.emoteSource === 'ambient') this.finishEmote();
    }
    if (ambientBlocked && frame.gesture) this.ambientGesture = frame.gesture.key;
    if (this.emoteSource === 'ambient' && this.emote && frame.gesture?.key === this.ambientGesture) {
      this.emote.start = this.time - frame.gesture.elapsed;
      if (this.emote.clip) {
        this.emote.clip.setEffectiveTimeScale(0);
        this.emote.clip.time = this.emote.clip.getClip().duration * frame.gesture.elapsed / frame.gesture.duration;
      }
    }
    let moving=false;
    if(this.desktopActive){
      const width=Math.max(1,this.host.clientWidth),height=Math.max(1,this.host.clientHeight);
      const scale=this.robot.root.scale.x;
      const units=height/(this.desktopCamera.top-this.desktopCamera.bottom);
      this.desktopFigure={width:Math.hypot(this.desktopModelBounds.size.x,this.desktopModelBounds.size.z)*scale*units,height:this.desktopModelBounds.size.y*scale*units};
      const movable=Boolean(config?.active&&config.wander&&this.canWalk());
      if(!movable)this.desktopMotion.reset();
      const position=(reduced||!movable)?{x:width/2,y:height/2,walking:false,facing:1,presentationReady:this.desktopFigure.width<=width-48&&this.desktopFigure.height<=height-48}:this.desktopMotion.sample({seed:config!.seed,startedAt:config!.startedAt,viewport:{width,height},figure:this.desktopFigure,margin:24,now:Date.now(),delta,paused:ambientBlocked||Boolean(this.emote),presentationRequested:presenting});
      this.desktopPresentationReady=position.presentationReady;
      moving=movable&&position.walking&&(presenting||!blocked&&!this.emote);
      this.setWalking(moving,!moving&&(blocked||presenting));this.walkStrength=moving?THREE.MathUtils.damp(this.walkStrength,1,14,delta):0;this.walkingPhase=frame.ageSeconds;
      const right=new THREE.Vector3(1,0,0),up=new THREE.Vector3(0,1,0);
      this.travel.position.set(0,0,0).addScaledVector(right,(position.x-width/2)/units).addScaledVector(up,(height/2-position.y)/units);

      const heading=moving?Math.atan2(right.x*position.facing,right.z*position.facing)-this.modelForwardYaw:0;
      const angle=Math.atan2(Math.sin(heading-this.travel.rotation.y),Math.cos(heading-this.travel.rotation.y));this.travel.rotation.y+=angle*(1-Math.exp(-delta*9));
      this.travel.position.sub(this.desktopModelBounds.center.clone().multiplyScalar(scale).applyQuaternion(this.travel.quaternion));
    }else{
    const scale = this.robot.root.scale.x;
    const radius = (Math.hypot(this.contentWidth, this.contentDepth) / 2 + this.contentHeight * 0.08) * scale;
    const lane = motionLaneHalfWidth(this.camera.position.distanceTo(this.controls.target), this.camera.fov, this.camera.aspect, radius);
    const capable = this.canWalk() && lane > 0.03;
    const previous = this.travelFraction;
    this.travelFraction = followAmbientPosition(previous, capable && config?.wander ? frame.x : previous, delta, ambientBlocked);
    moving = capable && Math.abs(this.travelFraction - previous) > 0.00001;
    this.setWalking(moving && !ambientBlocked, ambientBlocked);
    this.walkStrength = ambientBlocked ? 0 : THREE.MathUtils.damp(this.walkStrength, moving ? 1 : 0, 14, delta);
    this.walkingPhase = frame.ageSeconds;
    const right = new THREE.Vector3(1, 0, 0).applyQuaternion(this.camera.quaternion); right.y = 0; right.normalize();
    this.travel.position.copy(right).multiplyScalar(this.travelFraction * lane);
    const direction = this.travelFraction >= previous ? 1 : -1;
    const heading = moving && !ambientBlocked ? Math.atan2(right.x * direction, right.z * direction) - this.modelForwardYaw : 0;
    const angle = Math.atan2(Math.sin(heading - this.travel.rotation.y), Math.cos(heading - this.travel.rotation.y));
    this.travel.rotation.y += angle * (1 - Math.exp(-delta * 9));
    if (this.contactShadow) { this.contactShadow.position.x = this.travel.position.x; this.contactShadow.position.z = this.travel.position.z; }
    }
    if (this.desktopActive && !this.desktopPresentationReady && frame.gesture) this.ambientGesture = frame.gesture.key;
    if (this.walkingAction && this.walkingClip && this.walkingAction.enabled && this.walkingAction.isScheduled()) this.walkingAction.time = (frame.ageSeconds * 0.8) % this.walkingClip.duration;
    if (!ambientBlocked && (!this.desktopActive || this.desktopPresentationReady) && !moving && frame.gesture && frame.gesture.key !== this.ambientGesture && this.ambientActions.length) {
      this.ambientGesture = frame.gesture.key;
      const action = this.ambientActions[Math.min(this.ambientActions.length - 1, Math.floor(frame.gesture.choice * this.ambientActions.length))];
      this.triggerEmote(action, frame.gesture.duration * 1000, 0.8, 'ambient');
      if (this.emote) {
        this.emote.start = this.time - frame.gesture.elapsed;
        if (this.emote.clip) {
          this.emote.clip.setEffectiveTimeScale(0);
          this.emote.clip.time = this.emote.clip.getClip().duration * frame.gesture.elapsed / frame.gesture.duration;
        }
      }
    }
  }

  private animateWalkingGait(postMixer: boolean) {
    if (this.walkingClip || this.walkStrength < 0.001) return;
    const gait = sampleWalkingGait(this.walkingPhase);
    const strength = this.walkStrength;
    const { left, right, rotations } = gait;
    if (!this.asset && !postMixer) {
      this.robot.legs[0].rotation.x = -left * 0.28 * strength;
      this.robot.legs[1].rotation.x = -right * 0.28 * strength;
      this.robot.arms[0].rotation.x += left * 0.2 * strength;
      this.robot.arms[1].rotation.x += right * 0.2 * strength;
      const bob = gait.bob * strength;
      this.robot.torso.position.y += bob; this.robot.head.position.y += bob;
    }
    if (this.vrm && !postMixer) {
      for (const [name, x] of rotations) this.rotateHumanoid(name, x * strength);
      this.imported.position.y += gait.bob * strength;
    }
    const rig = this.armRig;
    if (!postMixer || !rig) return;
    for (const [name, x] of rotations) {
      const raw = this.vrm
        ? this.vrm.humanoid.getRawBoneNode(name)
        : name === 'leftUpperArm' ? rig.left.upper : name === 'rightUpperArm' ? rig.right.upper : this.walkingLegs?.[name as keyof Omit<WalkingLegs, 'rest'>];
      if (!raw || (this.vrm && !this.authored(raw, 'quaternion'))) continue;
      const offset = rotateGenericBone(rig, raw, x * strength, 0, 0);
      this.previousBoneOffsets.set(raw, offset);
    }
    if (!this.vrm) this.imported.position.y += gait.bob * strength;
  }

  private compensateWalkingRootMotion() {
    if (!this.walkAnchor || !this.walkingAction?.enabled || !this.walkingAction.isScheduled() || this.walkingAction.getEffectiveWeight() < 0.0001) return;
    const position = this.imported.worldToLocal(this.walkAnchor.getWorldPosition(new THREE.Vector3())).sub(this.walkAnchorRest);
    if (![position.x, position.y, position.z].every(Number.isFinite)) return;
    this.imported.position.x -= position.x * this.imported.scale.x;
    this.imported.position.z -= position.z * this.imported.scale.z;
    const vertical = THREE.MathUtils.clamp(position.y, -this.contentHeight * 0.025, this.contentHeight * 0.045);
    this.imported.position.y -= (position.y - vertical) * this.imported.scale.y;
  }

  private authored(node: THREE.Object3D, property: string, index?: number): boolean {
    return this.authoredProperties?.owns(this.activeAnimationActions, node, property, index) ?? false;
  }

  private triggerEmote(action: string, duration: number, intensity: number, source: 'manual' | 'ambient' = 'manual') {
    const resolved=resolveAvailableAction(this.getModelInfo().actions??[],action);
    if (!this.emoteReady || !resolved) { this.onEmoteError?.(unavailableEmote); return false; }
    action=resolved.action;
    this.setWalking(false);
    this.finishEmote();
    this.onEmoteError?.('');
    const active: ActiveEmote = {
      action,
      start: this.time,
      duration: THREE.MathUtils.clamp(Number.isFinite(duration) && duration > 0 ? duration / 1000 : 3, 0.4, 30),
      intensity: THREE.MathUtils.clamp(Number.isFinite(intensity) ? intensity : 1, 0.1, 2),
    };
    let clip: THREE.AnimationClip | undefined;
    try { clip = selectEmoteAnimation(this.clips, action); }
    catch (error) { this.onEmoteError?.(error instanceof Error ? error.message : String(error)); return; }
    if (clip || !['wave', 'hide', 'celebrate', 'nod', 'dance'].includes(action)) {
      if (!clip || !this.mixer) {
        this.onEmoteError?.(`Animation „${action}“ ist in dieser Figur nicht vorhanden.${this.clips.length ? ` Verfügbar: ${this.clipNames.join(', ')}` : ''}`);
        return;
      }
      const playing = this.mixer.clipAction(clip);
      if (isMotionAction(action)) active.duration = clip.duration;
      if (this.idleAction !== playing) this.idleAction?.fadeOut(0.15);
      playing.reset();
      playing.setLoop(THREE.LoopOnce, 1);
      playing.clampWhenFinished = true;
      playing.setEffectiveTimeScale(Math.max(0.01, clip.duration) / active.duration);
      playing.setEffectiveWeight(1);
      if (playing !== this.idleAction) playing.fadeIn(0.12);
      playing.play();
      active.clip = playing;
    }
    this.emote = active;
    this.emoteSource = source;
    return true;
  }

  private finishEmote() {
    const motion = Boolean(this.emote && isMotionAction(this.emote.action));
    if (this.emote?.clip) {
      const sharedIdle = this.emote.clip === this.idleAction;
      this.emote.clip.stop();
      if (this.idleAction && sharedIdle) {
        this.idleAction.reset().setLoop(THREE.LoopRepeat, Infinity);
        this.idleAction.setEffectiveTimeScale(1);
      }
      // The outgoing action is stopped now, so idle must already own the
      // complete pose. Natural endings cross-fade before reaching this point.
      this.resumeIdleAnimation();
    }
    this.emote = undefined;
    this.emoteSource = undefined;
    if (motion) this.vrm?.humanoid.resetNormalizedPose();
    this.robot.root.position.set(0, 0, 0);
    this.robot.root.rotation.set(0, 0, 0);
    this.imported.position.set(0, 0, 0);
    this.imported.rotation.set(0, 0, 0);
    for (const { node, quaternion } of this.humanoidPose.values()) node.quaternion.copy(quaternion);
    this.applyHumanoidRestPose();
  }

  private animateEmote(reduced: boolean) {
    const active = this.emote;
    if (!active) return;
    const elapsed = this.time - active.start;
    const progress = THREE.MathUtils.clamp(elapsed / active.duration, 0, 1);
    if (progress >= 1) { this.finishEmote(); return; }
    if (active.clip) {
      if (!active.fading && progress > 0.85) {
        if (active.clip !== this.idleAction) {
          const fade = Math.min(0.35, active.duration * 0.15);
          active.clip.fadeOut(fade);
          this.resumeIdleAnimation(fade);
        }
        active.fading = true;
      }
      return;
    }
    // Explicit emotes remain visible with reduced motion, but use a smaller,
    // slower pose and avoid jumps or repeated oscillation.
    const fade = THREE.MathUtils.smoothstep(progress, 0, 0.18) * (1 - THREE.MathUtils.smoothstep(progress, 0.78, 1));
    const weight = fade * active.intensity * (reduced ? 0.45 : 1);
    const cycle = reduced ? 0 : Math.sin(elapsed * 9);
    const r = this.robot;
    const bone = (name: string, x = 0, y = 0, z = 0, poseWeight = weight) => {
      const item = this.humanoidPose.get(name);
      if (!item) {
        if (name === 'head') this.pendingImportedHead = [x * poseWeight, y * poseWeight, z * poseWeight];
        return;
      }
      const rawHead = name === 'head' ? this.vrm?.humanoid.getRawBoneNode('head') : undefined;
      if (rawHead && this.authored(rawHead, 'quaternion')) this.pendingImportedHead = [x * poseWeight, y * poseWeight, z * poseWeight];
      const offset = humanoidRotation(this.humanoidBasis, x * poseWeight, y * poseWeight, z * poseWeight);
      item.node.quaternion.multiply(offset);
    };
    switch (active.action) {
      case 'wave': {
        r.arms[1].rotation.z += (2.1 + cycle * 0.23) * weight;
        r.arms[1].rotation.x -= 0.15 * weight;
        r.head.rotation.z -= 0.07 * weight;
        // Cap the raised pose at its target: doubled emote intensity must
        // not rotate a hand past the head and back down toward the floor.
        this.humanoidPose.get('rightUpperArm')?.node.quaternion.multiply(humanoidWaveRaise(this.humanoidBasis, weight));
        bone('rightLowerArm', 0, 0, -0.35);
        bone('rightHand', 0, cycle * 0.3, cycle * 0.23);
        bone('head', 0, 0, -0.07);
        break;
      }
      case 'hide': {
        const dip = -3.3 * weight;
        r.root.position.y = dip;
        this.imported.position.y = dip;
        r.head.rotation.x += 0.3 * weight;
        r.arms[0].rotation.z -= 0.4 * weight;
        r.arms[1].rotation.z += 0.4 * weight;
        bone('spine', 0.24);
        bone('head', 0.22);
        break;
      }
      case 'celebrate': {
        const jump = reduced ? 0 : Math.abs(Math.sin(elapsed * 5)) * 0.22 * weight;
        r.root.position.y = jump;
        this.imported.position.y = jump;
        r.arms[0].rotation.z -= 2.35 * weight;
        r.arms[1].rotation.z += 2.35 * weight;
        r.head.rotation.x -= 0.1 * weight;
        bone('leftUpperArm', 0, 0, 2.1, Math.min(weight, 1));
        bone('rightUpperArm', 0, 0, -2.1, Math.min(weight, 1));
        bone('leftLowerArm', 0, 0, 0.25);
        bone('rightLowerArm', 0, 0, -0.25);
        bone('head', -0.1);
        break;
      }
      case 'nod': {
        const nod = reduced ? 0.16 : Math.sin(elapsed * 7) * 0.22;
        r.head.rotation.x += nod * weight;
        bone('head', nod);
        break;
      }
      case 'dance': {
        const sway = reduced ? 0.06 : Math.sin(elapsed * 5) * 0.14;
        r.root.rotation.z = sway * weight;
        this.imported.rotation.z = sway * weight;
        r.root.position.x = sway * weight;
        this.imported.position.x = sway * weight;
        r.arms[0].rotation.z -= (0.8 + cycle * 0.3) * weight;
        r.arms[1].rotation.z += (0.8 - cycle * 0.3) * weight;
        r.head.rotation.z -= sway * weight;
        bone('spine', 0, Math.sin(elapsed * 4) * (reduced ? 0 : 0.12), sway);
        bone('leftUpperArm', 0, 0, 0.6 + cycle * 0.3);
        bone('rightUpperArm', 0, 0, -0.6 + cycle * 0.3);
        break;
      }
    }
  }

  private tiltImportedHead(x: number, y: number, z: number) {
    this.tiltImportedBone(this.genericHead, x, y, z);
  }

  private tiltImportedBone(node: THREE.Object3D | undefined, x: number, y: number, z: number, overrideIdle = false) {
    if (!node || (!overrideIdle && this.authored(node, 'quaternion'))) return;
    const delta = new THREE.Quaternion().setFromEuler(new THREE.Euler(x, y, z));
    node.quaternion.multiply(delta);
    const previous = this.previousBoneOffsets.get(node);
    if (previous) previous.multiply(delta);
    else this.previousBoneOffsets.set(node, delta);
  }

  private animateImportedEmote(reduced: boolean) {
    const active = this.emote;
    if (!active || active.clip) return;
    if (this.pendingImportedHead) {
      if (this.armRig) {
        const delta = rotateGenericBone(this.armRig, this.armRig.head, ...this.pendingImportedHead);
        this.previousBoneOffsets.set(this.armRig.head, delta);
      } else this.tiltImportedBone(this.genericHead, ...this.pendingImportedHead, true);
    }
    const rig = this.armRig;
    if (!rig) return;
    const elapsed = this.time - active.start;
    const progress = THREE.MathUtils.clamp(elapsed / active.duration, 0, 1);
    const fade = THREE.MathUtils.smoothstep(progress, 0, 0.18) * (1 - THREE.MathUtils.smoothstep(progress, 0.78, 1));
    const weight = fade * active.intensity * (reduced ? 0.45 : 1);
    const cycle = reduced ? 0 : Math.sin(elapsed * 9);
    const pose = (side: 'left' | 'right', upper: number, lower: number, wrist = 0) => {
      // VRM's normalized gestures already drive untracked arms. Fill only
      // those that an authored idle track would otherwise overwrite.
      if (this.vrm && !Object.values(rig[side]).some((node) => this.authored(node, 'quaternion'))) return;
      for (const [node, delta] of poseGenericArm(rig, side, upper, lower, weight, wrist)) this.previousBoneOffsets.set(node, delta);
    };
    if (active.action === 'wave') pose('right', THREE.MathUtils.degToRad(70), Math.PI / 2, cycle * 0.23);
    else if (active.action === 'celebrate') {
      pose('left', 1, 1.4); pose('right', 1, 1.4);
    } else if (active.action === 'dance') {
      pose('left', -0.3 + cycle * 0.3, -0.1 + cycle * 0.3);
      pose('right', -0.3 - cycle * 0.3, -0.1 - cycle * 0.3);
    }
  }

  private rotateHumanoid(name: string, x: number, y = 0, z = 0) {
    const node = this.humanoidPose.get(name)?.node;
    if (node) node.quaternion.multiply(humanoidRotation(this.humanoidBasis, x, y, z));
  }

  private applyHumanoidRestPose() {
    for (const side of ['left', 'right'] as const) {
      const arm = this.humanoidPose.get(`${side}UpperArm`);
      if (arm) arm.node.quaternion.multiply(humanoidArmRest(this.humanoidBasis, side));
    }
  }

  private hasMorphMotion(mesh: THREE.Mesh, index: number): boolean {
    const geometry = mesh.geometry;
    let cached = this.morphMotion.get(geometry);
    if (!cached) { cached = new Map(); this.morphMotion.set(geometry, cached); }
    const known = cached.get(index);
    if (known !== undefined) return known;
    const target = geometry.morphAttributes.position?.[index];
    const base = geometry.getAttribute('position');
    let changesPosition = false;
    if (target && base && target.count === base.count) {
      for (let i = 0; i < target.count; i++) {
        const x = target.getX(i) - (geometry.morphTargetsRelative ? 0 : base.getX(i));
        const y = target.getY(i) - (geometry.morphTargetsRelative ? 0 : base.getY(i));
        const z = target.getZ(i) - (geometry.morphTargetsRelative ? 0 : base.getZ(i));
        if (Math.max(Math.abs(x), Math.abs(y), Math.abs(z)) > 0.0000001) { changesPosition = true; break; }
      }
    }
    cached.set(index, changesPosition);
    return changesPosition;
  }

  private updateSpeech(dt: number, speaking: boolean, level: number) {
    const amplitude = speaking && Number.isFinite(level) ? THREE.MathUtils.clamp(level, 0, 1) : 0;
    // Raise quiet, real speech without opening the mouth during silent audio.
    // These are amplitude-driven vowel blends, not phoneme recognition.
    const target = Math.pow(amplitude, 0.65);
    this.mouthLevel = speaking
      ? THREE.MathUtils.damp(this.mouthLevel, target, target > this.mouthLevel ? 32 : 25, dt)
      : 0;
    if (amplitude > 0.005) this.speechPhase += dt * (3.4 + this.mouthLevel * 3);
    for (const vowel of VOWELS) this.vowelWeights[vowel] = 0;
    const position = this.speechPhase % VOWEL_SEQUENCE.length;
    const index = Math.floor(position);
    const blend = THREE.MathUtils.smoothstep(position - index, 0, 1);
    // Keep an open vowel in the blend while changing lip width and rounding.
    this.vowelWeights.aa = 0.45;
    this.vowelWeights[VOWEL_SEQUENCE[index]] += (1 - blend) * 0.55;
    this.vowelWeights[VOWEL_SEQUENCE[(index + 1) % VOWEL_SEQUENCE.length]] += blend * 0.55;
  }

  private animateSpeechJaw() {
    // Speech drives existing lips/mouth morphs and a real jaw only. Models
    // without a mouth rig do not substitute nodding or body movement.
    if (this.vrm) {
      const jaw = this.vrm.humanoid.getRawBoneNode('jaw');
      if (!this.mouthExpressions.size && jaw && this.armRig) {
        const offset = rotateGenericBone(this.armRig, jaw, this.mouthLevel * 0.32, 0, 0);
        this.previousBoneOffsets.set(jaw, offset);
      }
    } else if (!this.genericMouthRig) this.tiltImportedBone(this.jawBone, this.mouthLevel * 0.32, 0, 0, true);
  }

  private setVrmExpression(aliases: readonly string[], weight: number, mouthPriority = false) {
    const expression = aliases.map((name) => this.vrmExpressions.get(name)).find(Boolean);
    if (!expression || this.authored(expression, 'weight')) return;
    // VRM 1 emotions may explicitly block mouth expressions. While audible
    // speech is active, give the working mouth rig priority over that pose.
    expression.weight = mouthPriority && this.mouthLevel > 0.005 && expression.overrideMouth !== 'none'
      ? 0 : THREE.MathUtils.clamp(weight, 0, 1);
  }

  private animateVrmMouth() {
    let total = 0;
    let strongest = -1;
    let dominant: Vowel | undefined;
    for (const vowel of this.mouthExpressions.keys()) {
      total += this.vowelWeights[vowel];
      if (this.vowelWeights[vowel] > strongest) { strongest = this.vowelWeights[vowel]; dominant = vowel; }
    }
    for (const [vowel, expression] of this.mouthExpressions) {
      if (this.mouthLevel <= 0.005 && this.authored(expression, 'weight')) continue;
      const weight = this.vowelWeights[vowel];
      expression.weight = expression.isBinary
        ? (this.mouthLevel > 0.12 && vowel === dominant ? 1 : 0)
        : this.mouthLevel * (total > 0 ? weight / total : vowel === dominant ? 1 : 0);
    }
  }

  private animateImportedFace(dt: number, blink: number, emotion: string) {
    for (const mesh of this.faceMeshes) {
      const dictionary = mesh.morphTargetDictionary!;
      const influences = mesh.morphTargetInfluences!;
      const vowels = this.faceVowels.get(mesh)!;
      const available = new Set(vowels.values());
      const fallback = available.values().next().value;
      const total = [...available].reduce((sum, vowel) => sum + this.vowelWeights[vowel], 0);
      for (const [rawName, index] of Object.entries(dictionary)) {
        const name = expressionKey(rawName);
        const speakingMouth = this.mouthLevel > 0.005 && (vowels.has(index) || (/^(mouthopen|jawopen)$/.test(name) && this.hasMorphMotion(mesh, index)));
        if (!speakingMouth && this.authored(mesh, 'morphTargetInfluences', index)) continue;
        let target: number | undefined;
        const vowel = vowels.get(index);
        if (vowel) target = this.mouthLevel * (total > 0 ? this.vowelWeights[vowel] / total : vowel === fallback ? 1 : 0);
        else if (name === 'angry') target = /angry|wütend/i.test(emotion) ? 0.7 : 0;
        else if (name === 'sad') target = /sad|traurig/i.test(emotion) ? 0.65 : 0;
        else if (name === 'happy' || name === 'smile') target = /happy|joy|friendly|freu/i.test(emotion) ? 0.6 : 0;
        else if (name === 'surprised') {
          target = /surpris|überrasch/i.test(emotion) ? 0.65 : 0;
        } else if (/^(blink|blinkleft|blinkright|blinkl|blinkr|eyeblinkleft|eyeblinkright)$/.test(name)) target = Math.max(0, blink);
        else if (/^(mouthopen|jawopen)$/.test(name)) target = this.hasMorphMotion(mesh, index) ? this.mouthLevel : 0;
        if (target !== undefined) influences[index] = THREE.MathUtils.damp(influences[index] || 0, target, 18, dt);
      }
    }
  }

  /** Explicit root-space jumps/tilts must not move the framed figure past a
   * desktop edge. Ambient gestures are skipped offscreen instead of clamped. Keep intentional hide/entry movement, and do not alter the
   * stored camera or the controller's paused waypoint. Skeleton deformation
   * remains governed by the authored asset's bounds. */
  private clampDesktopEmote() {
    if (!this.desktopActive || !this.emote || this.emoteSource === 'ambient' || this.emote.action === 'hide') return;
    const root = this.asset ? this.imported : this.robot.root;
    root.updateWorldMatrix(true, true);
    const bounds = new THREE.Box3().setFromCenterAndSize(this.desktopModelBounds.center, this.desktopModelBounds.size).applyMatrix4(root.matrixWorld);
    const units = this.host.clientHeight / (this.desktopCamera.top - this.desktopCamera.bottom);
    const margin = 24 / units;
    const minX = this.desktopCamera.left + margin, maxX = this.desktopCamera.right - margin;
    const minY = this.desktopCamera.bottom + margin, maxY = this.desktopCamera.top - margin;
    const shift = (low: number, high: number, min: number, max: number) => high-low > max-min ? (min+max-low-high)/2 : low < min ? min-low : high > max ? max-high : 0;
    this.travel.position.x += shift(bounds.min.x, bounds.max.x, minX, maxX);
    this.travel.position.y += shift(bounds.min.y, bounds.max.y, minY, maxY);
    root.updateWorldMatrix(true, true);
  }

  private resize() {
    const width = Math.max(this.host.clientWidth, 1);
    const height = Math.max(this.host.clientHeight, 1);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.75));
    this.renderer.setSize(width, height, false);
    this.update(this.options);
  }

  private makeGround() {
    const textureCanvas = document.createElement('canvas');
    textureCanvas.width = 128;
    textureCanvas.height = 128;
    const ctx = textureCanvas.getContext('2d')!;
    const gradient = ctx.createRadialGradient(64, 64, 1, 64, 64, 64);
    gradient.addColorStop(0, 'rgba(55,53,93,0.35)');
    gradient.addColorStop(0.45, 'rgba(55,53,93,0.12)');
    gradient.addColorStop(1, 'rgba(55,53,93,0)');
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, 128, 128);
    const contact = new THREE.Mesh(new THREE.PlaneGeometry(3.2, 2.5), new THREE.MeshBasicMaterial({ map: new THREE.CanvasTexture(textureCanvas), transparent: true, depthWrite: false }));
    this.contactShadow = contact;
    contact.rotation.x = -Math.PI / 2;
    contact.position.y = 0.008;
    const shadow = new THREE.Mesh(new THREE.PlaneGeometry(20, 20), new THREE.ShadowMaterial({ color: 0x85839e, opacity: 0.16 }));
    shadow.rotation.x = -Math.PI / 2;
    shadow.receiveShadow = true;
    const halo = new THREE.Mesh(new THREE.RingGeometry(1.12, 1.125, 100), new THREE.MeshBasicMaterial({ color: 0xa8b8dc, transparent: true, opacity: 0.35, side: THREE.DoubleSide }));
    halo.rotation.x = -Math.PI / 2;
    halo.position.y = 0.012;
    this.ground.add(contact, shadow, halo);
  }

  private animate = (now = performance.now()) => {
    if (this.disposed) return;
    this.frame = requestAnimationFrame(this.animate);
    const interval = 1000 / (Number(this.options.profile?.output?.fps) === 30 ? 30 : 60);
    const sinceRender = now - this.lastRenderTime;
    if (sinceRender < interval - 0.5) return;
    this.lastRenderTime = now - (sinceRender % interval);
    const dt = Math.min(this.clock.getDelta(), 0.06);
    this.time += dt;
    const reduced = this.motion.matches;
    const { speaking, level, emotion, profile } = this.options;
    const idle = !reduced && profile?.avatar?.idle !== false;
    this.updateAutopilot(dt, reduced);
    const blinkEnabled = profile?.avatar?.autoBlink !== false;
    this.updateSpeech(dt, speaking, level);
    const t = this.time;
    const r = this.robot;
    r.root.position.set(0, 0, 0);
    r.root.rotation.set(0, 0, 0);
    this.imported.position.set(0, 0, 0);
    this.imported.rotation.set(0, 0, 0);
    for (const [node, offset] of this.previousBoneOffsets) node.quaternion.multiply(offset.invert());
    this.previousBoneOffsets.clear();
    this.pendingImportedHead = undefined;
    for (const [node, quaternion] of this.genericBoneRest) node.quaternion.copy(quaternion);
    if (!this.vrm && this.walkingLegs) for (const [node, quaternion] of this.walkingLegs.rest) node.quaternion.copy(quaternion);
    if (!this.vrm && this.armRig) {
      for (const [node, quaternion] of this.armRig.rest) node.quaternion.copy(quaternion);
      if (!this.idleAction) for (const side of ['left', 'right'] as const) poseGenericArm(this.armRig, side, -1.15, -1.15, 1);
    }
    for (const { node, quaternion } of this.humanoidPose.values()) node.quaternion.copy(quaternion);
    this.applyHumanoidRestPose();
    const breath = idle ? Math.sin(t * 1.5) : 0;
    r.torso.position.y = breath * 0.013;
    r.torso.rotation.set(0, 0, 0);
    r.head.position.y = 2.24 + breath * 0.016;
    r.head.rotation.y = 0;
    r.head.rotation.z = 0;
    const cheerful = /happy|joy|excited|greet|friendly|freud|fröh|begrü|freundlich/i.test(emotion);
    r.head.rotation.x = 0;
    r.arms[0].rotation.z = 0.04;
    r.arms[1].rotation.z = -0.04;
    r.arms[0].rotation.x = r.arms[1].rotation.x = 0;
    for (const leg of r.legs) leg.rotation.set(0, 0, 0);
    if (t > this.blinkAt + 0.16) this.blinkAt = t + 2.7 + Math.random() * 3.5;
    const blink = blinkEnabled && t >= this.blinkAt ? Math.sin((t - this.blinkAt) / 0.16 * Math.PI) : 0;
    for (const eye of r.eyes) {
      eye.scale.y = Math.max(0.055, (cheerful ? 0.82 : 1) * (1 - blink));
      eye.position.x = eye.userData.baseX + (idle ? Math.sin(t * 0.44) * 0.018 : 0);
    }
    r.mouth.scale.y = 1 + this.mouthLevel * 4;
    r.mouth.scale.x = 1 - this.mouthLevel * 0.15;
    r.jaw.position.y = -0.35 - this.mouthLevel * 0.045;
    (r.antenna.material as THREE.MeshStandardMaterial).emissiveIntensity = 0.5 + (idle ? Math.sin(t * 1.8) * 0.15 : 0);
    this.animateEmote(reduced);
    this.animateWalkingGait(false);
    // Establish procedural defaults first. Authored skeleton tracks then win;
    // the final VRM update must not transfer the humanoid a second time.
    this.vrm?.humanoid.update();
    if (this.mixer) {
      if (this.idleAction) this.idleAction.paused = !idle && this.emote?.clip !== this.idleAction;
      this.authoredProperties?.restoreTransforms(this.activeAnimationActions);
      this.mixer.update(dt);
      this.authoredProperties?.captureTransforms(this.activeAnimationActions);
      if (this.vrm && this.emote?.clip && isMotionAction(this.emote.action)) transferMotionPose(this.vrm,this.emote.clip.getClip());
    }
    this.animateSpeechJaw();
    this.animateWalkingGait(true);
    this.animateImportedEmote(reduced);
    this.animateImportedFace(dt, blink, emotion);
    if (this.vrm) {
      if (this.vrmExpressions.has('blink')) this.setVrmExpression(['blink'], Math.max(0, blink));
      else {
        this.setVrmExpression(['blinkleft', 'blinkl'], Math.max(0, blink));
        this.setVrmExpression(['blinkright', 'blinkr'], Math.max(0, blink));
      }
      this.animateVrmMouth();
      this.setVrmExpression(['happy', 'joy'], cheerful || this.emote?.action === 'celebrate' ? 0.6 : 0, true);
      this.setVrmExpression(['sad', 'sorrow'], /sad|traurig/i.test(emotion) ? 0.6 : 0, true);
      this.setVrmExpression(['angry'], /angry|wütend/i.test(emotion) ? 0.4 : 0, true);
      this.setVrmExpression(['surprised', 'surprise'], /surpris|überrasch/i.test(emotion) ? 0.55 : 0, true);
      this.lookAt.position.copy(this.activeCamera.position);
      const autoLookAt = this.vrm.lookAt?.autoUpdate;
      if (this.vrm.lookAt && this.motionLookAt && this.authored(this.motionLookAt,'quaternion')) this.vrm.lookAt.autoUpdate = false;
      try { finishVrmAnimationFrame(this.vrm, dt, this.authoredProperties, this.activeAnimationActions, this.mouthLevel > 0.005 ? [...this.mouthExpressions.values()] : []); }
      finally { if (this.vrm.lookAt && autoLookAt !== undefined) this.vrm.lookAt.autoUpdate = autoLookAt; }
    }
    this.compensateWalkingRootMotion();
    this.clampDesktopEmote();
    this.controls.update();
    if(now-this.lastDesktopAnchor>100){
      this.lastDesktopAnchor=now;
      if(this.desktopActive){const root=this.asset?this.imported:this.robot.root;const center=root.localToWorld(this.desktopModelBounds.center.clone()).project(this.activeCamera);const width=this.host.clientWidth,height=this.host.clientHeight;const x=(center.x+1)*width/2,y=(1-center.y)*height/2;this.options.onDesktopAnchor?.({x,y,...this.desktopFigure,visible:center.z>=-1&&center.z<=1&&x+this.desktopFigure.width/2>0&&x-this.desktopFigure.width/2<width&&y+this.desktopFigure.height/2>0&&y-this.desktopFigure.height/2<height,viewport:{width,height}});}else this.options.onDesktopAnchor?.(undefined);
    }
    try {
      this.renderer.render(this.scene, this.activeCamera);
      if (this.presentation.requested) {
        const hostVisible = this.host.isConnected && this.host.clientWidth > 0 && this.host.clientHeight > 0 && getComputedStyle(this.host).visibility !== 'hidden';
        const loaded = this.emoteReady && JSON.stringify(avatarBinding(this.options.profile)) === this.loadedEmoteBinding;
        const ready = hostVisible && !this.renderer.getContext().isContextLost() && this.assetState === 'ready' && loaded && !this.emote && !this.walking && this.walkStrength < 0.001 && (!this.desktopActive || this.desktopPresentationReady);
        this.presentation.rendered(ready);
      }
    } catch (error) { this.presentation.fail(error); }
  };

  private clearImported() {
    this.desktopMotion.reset();
    this.emoteReady = false;
    this.loadedEmoteBinding = '';
    this.motionAbort?.abort();
    this.motionAbort = undefined;
    this.motionLookAt = undefined;
    this.finishEmote();
    this.desktopModelBounds=measureDesktopModel(this.robot.root);
    this.mixer?.stopAllAction();
    if (this.mixer) this.mixer.uncacheRoot(this.mixer.getRoot());
    this.mixer = undefined;
    this.authoredProperties = undefined;
    this.clips = [];
    this.walkingClip = undefined;
    this.walkingAction = undefined;
    this.walking = false;
    this.walkStrength = 0;
    this.walkingLegs = undefined;
    this.walkAnchor = undefined;
    this.walkAnchorRest.set(0, 0, 0);
    this.modelForwardYaw = 0;
    this.contentWidth = 2.05;
    this.contentDepth = 1.35;
    this.travelFraction = 0;
    this.ambientSession = '';
    this.ambientGesture = '';
    this.ambientActions = ['wave', 'nod', 'celebrate'];
    this.travel.position.set(0, 0, 0);
    this.travel.rotation.set(0, 0, 0);
    this.idleAction = undefined;
    this.humanoidPose.clear();
    this.humanoidBasis.identity();
    this.faceMeshes = [];
    this.faceVowels.clear();
    this.genericMouthRig = false;
    this.vrmExpressions.clear();
    this.mouthExpressions.clear();
    this.morphMotion = new WeakMap();
    this.genericHead = undefined;
    this.genericChest = undefined;
    this.armRig = undefined;
    this.genericBoneRest.clear();
    this.pendingImportedHead = undefined;
    this.jawBone = undefined;
    this.previousBoneOffsets.clear();
    this.contentHeight = 2.9;
    this.vrm = undefined;
    for (const object of [...this.imported.children]) {
      this.imported.remove(object);
      disposeObject(object);
    }
  }

  dispose() {
    this.disposed = true;
    this.presentation.dispose();
    ++this.request;
    cancelAnimationFrame(this.frame);
    this.observer.disconnect();
    this.controls.dispose();
    this.clearImported();
    disposeObject(this.scene);
    this.assetLoader.dispose();
    this.renderer.dispose();
    this.renderer.forceContextLoss();
    this.renderer.domElement.remove();
  }
}


export function createRobot(): Robot {
  const root = new THREE.Group();
  const torso = new THREE.Group();
  root.add(torso);
  const ivory = new THREE.MeshStandardMaterial({ color: 0xf5f2e9, roughness: 0.29, metalness: 0.11 });
  const porcelain = new THREE.MeshStandardMaterial({ color: 0xfffdf7, roughness: 0.24, metalness: 0.07 });
  const seams = new THREE.MeshStandardMaterial({ color: 0x555b76, roughness: 0.45, metalness: 0.45 });
  const dark = new THREE.MeshStandardMaterial({ color: 0x182839, roughness: 0.23, metalness: 0.45 });
  const visor = new THREE.MeshPhysicalMaterial({ color: 0x14283d, roughness: 0.17, metalness: 0.2, clearcoat: 1, clearcoatRoughness: 0.12 });
  const accent = new THREE.MeshStandardMaterial({ color: 0x80b8e8, roughness: 0.3, metalness: 0.36 });
  const lavender = new THREE.MeshStandardMaterial({ color: 0xa69bd8, roughness: 0.38, metalness: 0.18 });
  const led = new THREE.MeshStandardMaterial({ color: 0xa8e0ff, emissive: 0x80b8e8, emissiveIntensity: 1.3, roughness: 0.35, toneMapped: false });
  const white = new THREE.MeshBasicMaterial({ color: 0xf5fbff });

  function mesh(geometry: THREE.BufferGeometry, material: THREE.Material, parent: THREE.Object3D, position: [number, number, number] = [0, 0, 0]) {
    const object = new THREE.Mesh(geometry, material);
    object.position.set(...position);
    object.castShadow = true;
    object.receiveShadow = true;
    parent.add(object);
    return object;
  }
  const round = (width: number, height: number, depth: number, radius = 0.1) => new RoundedBoxGeometry(width, height, depth, 5, radius);
  const sphere = (radius: number) => new THREE.SphereGeometry(radius, 40, 28);

  // Separate hip pivots preserve the neutral geometry and allow real leg
  // swings for the optional stage-only walking cycle.
  const legs: THREE.Group[] = [];
  for (const side of [-1, 1]) {
    const leg = new THREE.Group(); leg.position.set(side * 0.26, 0.8, 0); root.add(leg); legs.push(leg);
    mesh(round(0.46, 0.23, 0.62, 0.1), porcelain, leg, [side * 0.02, -0.67, 0.12]);
    mesh(round(0.39, 0.035, 0.5, 0.016), lavender, leg, [side * 0.02, -0.755, 0.13]);
    mesh(new THREE.CylinderGeometry(0.14, 0.16, 0.4, 32), seams, leg, [0, -0.39, 0]);
    mesh(round(0.34, 0.37, 0.37, 0.12), ivory, leg, [0, -0.32, 0.02]);
    const knee = mesh(new THREE.CylinderGeometry(0.085, 0.085, 0.035, 32), accent, leg, [0, -0.28, 0.215]);
    knee.rotation.x = Math.PI / 2;
  }
  mesh(round(0.77, 0.33, 0.53, 0.13), seams, torso, [0, 0.76, 0]);
  mesh(round(1.02, 0.91, 0.67, 0.23), ivory, torso, [0, 1.23, 0]);
  mesh(round(0.76, 0.62, 0.08, 0.13), porcelain, torso, [0, 1.25, 0.325]);
  mesh(round(0.26, 0.09, 0.035, 0.03), accent, torso, [0, 1.39, 0.38]);
  const chestIcon = mesh(new THREE.TorusGeometry(0.055, 0.009, 12, 40), led, torso, [0, 1.39, 0.406]);
  chestIcon.scale.setScalar(0.65);
  for (let i = 0; i < 3; i++) mesh(round(0.09, 0.016, 0.018, 0.007), seams, torso, [-0.12 + i * 0.12, 1.03, 0.374]);
  mesh(new THREE.CylinderGeometry(0.18, 0.22, 0.23, 40), seams, torso, [0, 1.77, 0]);
  mesh(new THREE.TorusGeometry(0.19, 0.027, 12, 48), accent, torso, [0, 1.79, 0]).rotation.x = Math.PI / 2;

  const arms: THREE.Group[] = [];
  for (const side of [-1, 1]) {
    const arm = new THREE.Group();
    arm.position.set(side * 0.59, 1.56, 0);
    torso.add(arm);
    arms.push(arm);
    mesh(sphere(0.17), seams, arm);
    const shoulder = mesh(round(0.25, 0.3, 0.35, 0.1), porcelain, arm, [side * 0.035, -0.02, 0]);
    shoulder.rotation.z = side * 0.12;
    const cuff = mesh(new THREE.CylinderGeometry(0.1, 0.1, 0.035, 32), accent, arm, [side * 0.16, -0.05, 0]);
    cuff.rotation.z = Math.PI / 2;
    mesh(round(0.23, 0.38, 0.27, 0.1), ivory, arm, [side * 0.055, -0.29, 0]);
    mesh(sphere(0.115), seams, arm, [side * 0.07, -0.49, 0]);
    mesh(round(0.25, 0.32, 0.29, 0.1), ivory, arm, [side * 0.075, -0.64, 0.03]);
    mesh(new THREE.CylinderGeometry(0.112, 0.112, 0.06, 32), lavender, arm, [side * 0.075, -0.78, 0.03]);
    mesh(round(0.245, 0.24, 0.28, 0.1), porcelain, arm, [side * 0.075, -0.91, 0.06]);
    mesh(sphere(0.068), porcelain, arm, [side * -0.04, -0.87, 0.17]);
    for (let i = 0; i < 2; i++) mesh(round(0.012, 0.07, 0.015, 0.005), seams, arm, [side * 0.075 + (i - 0.5) * 0.075, -0.95, 0.201]);
  }

  const head = new THREE.Group();
  head.position.set(0, 2.24, 0);
  root.add(head);
  mesh(round(1.34, 0.99, 0.94, 0.25), ivory, head);
  mesh(round(1.19, 0.69, 0.085, 0.19), seams, head, [0, 0.015, 0.458]);
  mesh(round(1.12, 0.61, 0.095, 0.17), visor, head, [0, 0.025, 0.5]);
  // Slightly offset glass highlight follows the visor's top edge.
  const sheen = mesh(round(0.77, 0.014, 0.007, 0.006), new THREE.MeshBasicMaterial({ color: 0x7b99b3, transparent: true, opacity: 0.24 }), head, [-0.08, 0.26, 0.551]);
  sheen.rotation.z = 0.01;
  const eyes: THREE.Group[] = [];
  for (const side of [-1, 1]) {
    const eye = new THREE.Group();
    eye.position.set(side * 0.26, 0.065, 0.559);
    eye.userData.baseX = side * 0.26;
    head.add(eye);
    mesh(round(0.13, 0.205, 0.027, 0.06), led, eye);
    mesh(round(0.025, 0.055, 0.008, 0.012), white, eye, [-0.027, 0.037, 0.017]);
    eyes.push(eye);
    mesh(sphere(0.023), accent, head, [side * 0.435, -0.13, 0.553]);
    const ear = mesh(new THREE.CylinderGeometry(0.245, 0.245, 0.14, 48), porcelain, head, [side * 0.709, -0.005, -0.015]);
    ear.rotation.z = Math.PI / 2;
    const earInset = mesh(new THREE.CylinderGeometry(0.175, 0.175, 0.025, 48), lavender, head, [side * 0.79, -0.005, -0.015]);
    earInset.rotation.z = Math.PI / 2;
    const earCore = mesh(new THREE.CylinderGeometry(0.1, 0.1, 0.031, 48), accent, head, [side * 0.807, -0.005, -0.015]);
    earCore.rotation.z = Math.PI / 2;
  }
  const mouth = new THREE.Group();
  mouth.position.set(0, -0.135, 0.56);
  head.add(mouth);
  for (let i = 0; i < 7; i++) {
    const x = (i - 3) * 0.033;
    mesh(round(0.024, 0.018, 0.012, 0.008), led, mouth, [x, x * x * 2.0, 0]);
  }
  const jaw = mesh(round(0.73, 0.16, 0.12, 0.06), porcelain, head, [0, -0.35, 0.44]);
  mesh(round(0.22, 0.025, 0.014, 0.009), lavender, head, [0, -0.36, 0.506]);
  mesh(new THREE.CylinderGeometry(0.036, 0.045, 0.18, 24), seams, head, [0.35, 0.565, -0.15]);
  const antenna = mesh(sphere(0.075), led, head, [0.35, 0.68, -0.15]);
  return { root, head, torso, arms, legs, eyes, mouth, jaw, antenna, accents: [accent, led] };
}
