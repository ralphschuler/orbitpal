import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';
import { VRMExpressionMorphTargetBind, VRMLoaderPlugin, VRMUtils, type VRM, type VRMExpression } from '@pixiv/three-vrm';

export interface AvatarOptions {
  profile: any;
  speaking: boolean;
  level: number;
  emotion: string;
  transparent: boolean;
}

type Robot = {
  root: THREE.Group;
  head: THREE.Group;
  torso: THREE.Group;
  arms: THREE.Group[];
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
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(32, 1, 0.05, 100);
  private controls: OrbitControls;
  private clock = new THREE.Clock();
  private robot: Robot;
  private ground = new THREE.Group();
  private imported = new THREE.Group();
  private vrm?: VRM;
  private mixer?: THREE.AnimationMixer;
  private clips: THREE.AnimationClip[] = [];
  private idleAction?: THREE.AnimationAction;
  private emote?: ActiveEmote;
  private emoteNonce: string | number | undefined;
  private humanoidPose = new Map<string, { node: THREE.Object3D; quaternion: THREE.Quaternion }>();
  private faceMeshes: THREE.Mesh[] = [];
  private faceVowels = new Map<THREE.Mesh, Map<number, Vowel>>();
  private genericMouthRig = false;
  private vrmExpressions = new Map<string, VRMExpression>();
  private mouthExpressions = new Map<Vowel, VRMExpression>();
  private morphMotion = new WeakMap<THREE.BufferGeometry, Map<number, boolean>>();
  private genericHead?: THREE.Object3D;
  private genericChest?: THREE.Object3D;
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
  private cameraMode = '';
  private time = 0;
  private mouthLevel = 0;
  private speechPhase = 0;
  private vowelWeights: Record<Vowel, number> = { aa: 0, ih: 0, ou: 0, ee: 0, oh: 0 };
  private blinkAt = 2.4;
  private previousEmotion = '';
  private gestureAt = -10;

  constructor(private host: HTMLElement, options: AvatarOptions, private onEmoteError?: (message: string) => void) {
    this.options = options;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'high-performance' });
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
    this.scene.add(this.robot.root, this.imported, this.ground, this.lookAt);
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
    this.options = options;
    const avatar = options.profile?.avatar ?? {};
    this.ground.visible = !options.transparent;
    const scale = THREE.MathUtils.clamp(Number(avatar.scale) || 1, 0.3, 2.5);
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
    if (this.previousEmotion !== options.emotion) {
      this.previousEmotion = options.emotion;
      this.gestureAt = this.time;
    }
    const emote = avatar.emote;
    if (!emote && this.emoteNonce !== undefined) {
      this.emoteNonce = undefined;
      this.finishEmote();
    }
    if (emote && emote.nonce !== undefined && emote.nonce !== this.emoteNonce) {
      this.emoteNonce = emote.nonce;
      this.triggerEmote(String(emote.action || ''), Number(emote.duration), Number(emote.intensity));
    }
  }

  async loadAsset(url?: string): Promise<void> {
    const next = url || '';
    if (next === this.asset) return;
    this.asset = next;
    const request = ++this.request;
    this.clearImported();
    this.robot.root.visible = true;
    this.cameraMode = '';
    this.update(this.options);
    if (!next) return;
    try {
      // GLTFLoader can swallow a failed image request and finish with an
      // untextured material. Treat missing resources as an import error.
      let resourceFailed = false;
      const manager = new THREE.LoadingManager();
      manager.onError = () => { resourceFailed = true; };
      const loader = new GLTFLoader(manager);
      loader.register((parser) => new VRMLoaderPlugin(parser));
      const gltf = await loader.loadAsync(next);
      if (this.disposed || request !== this.request) {
        disposeObject(gltf.scene);
        return;
      }
      if (resourceFailed) {
        disposeObject(gltf.scene);
        throw new Error('Texturen oder Modelldaten konnten nicht vollständig geladen werden.');
      }
      const vrm = gltf.userData.vrm as VRM | undefined;
      if (vrm) {
        VRMUtils.removeUnnecessaryVertices(gltf.scene);
        VRMUtils.combineSkeletons(gltf.scene);
        VRMUtils.rotateVRM0(vrm);
        vrm.lookAt && (vrm.lookAt.target = this.lookAt);
      }
      const normalization = new THREE.Group();
      normalization.add(gltf.scene);
      if (new URL(next, window.location.href).pathname.endsWith('/characters/Fox.glb')) normalization.rotation.y = Math.PI * 0.23;
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
      });
      this.imported.add(normalization);
      this.vrm = vrm;
      if (vrm) {
        // Normalized VRM bones have a common T-pose basis. These procedural
        // gestures are an approximation, not full animation retargeting.
        for (const name of ['head', 'neck', 'spine', 'chest', 'hips', 'jaw', 'leftUpperArm', 'rightUpperArm', 'leftLowerArm', 'rightLowerArm', 'leftHand', 'rightHand'] as const) {
          const node = vrm.humanoid.getNormalizedBoneNode(name);
          if (!node) continue;
          if (name === 'leftUpperArm') node.rotation.z = -1.15;
          if (name === 'rightUpperArm') node.rotation.z = 1.15;
          this.humanoidPose.set(name, { node, quaternion: node.quaternion.clone() });
        }
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
      if (gltf.animations.length) {
        this.mixer = new THREE.AnimationMixer(gltf.scene);
        // Prefer the neutral action over e.g. 2H_Melee_Idle or Sit_Chair_Idle
        // in packs containing many activity-specific idle animations.
        const clip = gltf.animations.find((animation) => /^idle$/i.test(animation.name))
          ?? gltf.animations.find((animation) => /^unarmed_idle$/i.test(animation.name))
          ?? gltf.animations.find((animation) => /^survey$/i.test(animation.name))
          ?? gltf.animations.find((animation) => /^(rest|stand|standing|idle_b)$/i.test(animation.name))
          ?? gltf.animations.find((animation) => /(?:^|[_ .-])idle(?:$|[_ .-])/i.test(animation.name));
        if (clip) this.idleAction = this.mixer.clipAction(clip).play();
      }
      this.robot.root.visible = false;
      this.cameraMode = '';
      this.update(this.options);
    } catch (error) {
      if (request !== this.request || this.disposed) return;
      this.robot.root.visible = true;
      throw error;
    }
  }

  get clipNames(): string[] { return this.clips.map((clip) => clip.name); }

  private triggerEmote(action: string, duration: number, intensity: number) {
    this.finishEmote();
    this.onEmoteError?.('');
    if (!action) return;
    const active: ActiveEmote = {
      action,
      start: this.time,
      duration: THREE.MathUtils.clamp(Number.isFinite(duration) && duration > 0 ? duration / 1000 : 3, 0.4, 30),
      intensity: THREE.MathUtils.clamp(Number.isFinite(intensity) ? intensity : 1, 0.1, 2),
    };
    const aliases: Record<string, string[]> = { wave: ['Wave'], celebrate: ['Cheer', 'Jump', 'Jump_Full_Short'], nod: ['Yes'], dance: ['Dance'] };
    const clip = this.clips.find((item) => item.name === action)
      ?? this.clips.find((item) => item.name.toLowerCase() === action.toLowerCase())
      ?? this.clips.find((item) => (aliases[action] || []).includes(item.name));
    if (clip || !['wave', 'hide', 'celebrate', 'nod', 'dance'].includes(action)) {
      if (!clip || !this.mixer) {
        this.onEmoteError?.(`Animation „${action}“ ist in dieser Figur nicht vorhanden.${this.clips.length ? ` Verfügbar: ${this.clipNames.join(', ')}` : ''}`);
        return;
      }
      const playing = this.mixer.clipAction(clip);
      if (this.idleAction !== playing) this.idleAction?.fadeOut(0.15);
      playing.reset();
      playing.setLoop(THREE.LoopOnce, 1);
      playing.clampWhenFinished = true;
      playing.setEffectiveTimeScale(Math.max(0.01, clip.duration) / active.duration);
      playing.setEffectiveWeight(1).fadeIn(0.12).play();
      active.clip = playing;
    }
    this.emote = active;
  }

  private finishEmote() {
    if (this.emote?.clip) {
      this.emote.clip.stop();
      if (this.idleAction) {
        this.idleAction.reset().setLoop(THREE.LoopRepeat, Infinity);
        this.idleAction.setEffectiveTimeScale(1).setEffectiveWeight(1).fadeIn(0.18).play();
      }
    }
    this.emote = undefined;
    this.robot.root.position.set(0, 0, 0);
    this.robot.root.rotation.set(0, 0, 0);
    this.imported.position.set(0, 0, 0);
    this.imported.rotation.set(0, 0, 0);
    for (const { node, quaternion } of this.humanoidPose.values()) node.quaternion.copy(quaternion);
  }

  private animateEmote(reduced: boolean) {
    const active = this.emote;
    if (!active) return;
    const elapsed = this.time - active.start;
    const progress = THREE.MathUtils.clamp(elapsed / active.duration, 0, 1);
    if (progress >= 1) { this.finishEmote(); return; }
    if (active.clip) {
      if (!active.fading && progress > 0.85) {
        active.clip.fadeOut(Math.min(0.35, active.duration * 0.15));
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
    const bone = (name: string, x = 0, y = 0, z = 0) => {
      const item = this.humanoidPose.get(name);
      if (!item) {
        if (name === 'head') this.tiltImportedHead(x * weight, y * weight, z * weight);
        return;
      }
      const offset = new THREE.Quaternion().setFromEuler(new THREE.Euler(x * weight, y * weight, z * weight));
      item.node.quaternion.multiply(offset);
    };
    switch (active.action) {
      case 'wave':
        r.arms[1].rotation.z += (2.1 + cycle * 0.23) * weight;
        r.arms[1].rotation.x -= 0.15 * weight;
        r.head.rotation.z -= 0.07 * weight;
        bone('rightUpperArm', -0.2, 0, -2.0);
        bone('rightLowerArm', 0, 0, -0.35);
        bone('rightHand', 0, cycle * 0.3, cycle * 0.23);
        bone('head', 0, 0, -0.07);
        break;
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
        bone('leftUpperArm', 0, 0, 2.1);
        bone('rightUpperArm', 0, 0, -2.1);
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

  private tiltImportedBone(node: THREE.Object3D | undefined, x: number, y: number, z: number) {
    if (!node) return;
    const delta = new THREE.Quaternion().setFromEuler(new THREE.Euler(x, y, z));
    node.quaternion.multiply(delta);
    const previous = this.previousBoneOffsets.get(node);
    if (previous) previous.multiply(delta);
    else this.previousBoneOffsets.set(node, delta);
  }

  private rotateHumanoid(name: string, x: number, y = 0, z = 0) {
    const node = this.humanoidPose.get(name)?.node;
    if (node) node.quaternion.multiply(new THREE.Quaternion().setFromEuler(new THREE.Euler(x, y, z)));
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

  private animateSpeechGesture(reduced: boolean) {
    const amount = this.mouthLevel * (reduced ? 0.25 : 1) * (this.emote ? 0.45 : 1);
    if (amount < 0.0001) return;
    const beat = Math.sin(this.speechPhase * 1.35);
    const sway = Math.sin(this.speechPhase * 0.57);
    const noMouthRig = this.vrm ? !this.mouthExpressions.size && !this.jawBone : !this.genericMouthRig && !this.jawBone;
    const emphasis = noMouthRig ? 1.55 : 1;
    const nod = (0.035 + beat * 0.06) * amount * emphasis;
    const tilt = sway * 0.045 * amount * emphasis;
    const r = this.robot;
    r.head.rotation.x += nod;
    r.head.rotation.z += tilt;
    r.torso.rotation.x = amount * 0.025;
    r.torso.rotation.z = tilt * 0.35;
    r.arms[0].rotation.x -= (0.07 + beat * 0.045) * amount;
    r.arms[1].rotation.x -= (0.1 - beat * 0.045) * amount;
    if (this.vrm) {
      this.rotateHumanoid('head', nod, sway * amount * 0.025, tilt);
      this.rotateHumanoid(this.humanoidPose.has('chest') ? 'chest' : 'spine', amount * 0.025, 0, tilt * 0.45);
      this.rotateHumanoid('leftUpperArm', -(0.07 + beat * 0.045) * amount);
      this.rotateHumanoid('rightUpperArm', -(0.1 - beat * 0.045) * amount);
      this.rotateHumanoid('leftLowerArm', -amount * 0.09);
      this.rotateHumanoid('rightLowerArm', -amount * 0.12);
      if (!this.mouthExpressions.size && this.jawBone) this.rotateHumanoid('jaw', this.mouthLevel * 0.32);
    } else {
      this.tiltImportedHead(nod, sway * amount * 0.025, tilt);
      this.tiltImportedBone(this.genericChest, amount * 0.03, 0, tilt * 0.45);
      if (!this.genericMouthRig) this.tiltImportedBone(this.jawBone, this.mouthLevel * 0.32, 0, 0);
      if (!this.genericHead && !this.genericChest) {
        // Unrigged figures still give visible, audio-synchronous feedback.
        // Their original geometry is preserved; no artificial face is added.
        this.imported.rotation.z += tilt * 0.65;
        this.imported.rotation.x += nod * 0.4;
        this.imported.position.y += amount * (0.012 + Math.max(0, beat) * 0.018);
      }
    }
  }

  private setVrmExpression(aliases: readonly string[], weight: number, mouthPriority = false) {
    const expression = aliases.map((name) => this.vrmExpressions.get(name)).find(Boolean);
    if (!expression) return;
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
      const hasMouth = vowels.size > 0 || Object.entries(dictionary).some(([name, index]) => /^(mouthopen|jawopen)$/.test(expressionKey(name)) && this.hasMorphMotion(mesh, index));
      for (const [rawName, index] of Object.entries(dictionary)) {
        const name = expressionKey(rawName);
        let target: number | undefined;
        const vowel = vowels.get(index);
        if (vowel) target = this.mouthLevel * (total > 0 ? this.vowelWeights[vowel] / total : vowel === fallback ? 1 : 0);
        else if (name === 'angry') target = /angry|wütend/i.test(emotion) ? 0.7 : 0;
        else if (name === 'sad') target = /sad|traurig/i.test(emotion) ? 0.65 : 0;
        else if (name === 'happy' || name === 'smile') target = /happy|joy|friendly|freu/i.test(emotion) ? 0.6 : 0;
        else if (name === 'surprised') {
          // Robot Expressive has expressive face morphs but no phoneme rig.
          // This is audio-reactive expression, not claimed as exact lip sync.
          target = Math.max(/surpris|überrasch/i.test(emotion) ? 0.65 : 0, hasMouth ? 0 : this.mouthLevel * 0.9);
        } else if (/^(blink|eyeblinkleft|eyeblinkright)$/.test(name)) target = Math.max(0, blink);
        else if (/^(mouthopen|jawopen)$/.test(name)) target = this.hasMorphMotion(mesh, index) ? this.mouthLevel : 0;
        if (target !== undefined) influences[index] = THREE.MathUtils.damp(influences[index] || 0, target, 18, dt);
      }
    }
  }

  private resize() {
    const width = Math.max(this.host.clientWidth, 1);
    const height = Math.max(this.host.clientHeight, 1);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(width, height, false);
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
    for (const { node, quaternion } of this.humanoidPose.values()) node.quaternion.copy(quaternion);
    const breath = idle ? Math.sin(t * 1.5) : 0;
    r.torso.position.y = breath * 0.013;
    r.torso.rotation.set(0, 0, 0);
    r.head.position.y = 2.24 + breath * 0.016;
    r.head.rotation.y = idle ? Math.sin(t * 0.6) * 0.055 : 0;
    r.head.rotation.z = idle ? Math.sin(t * 0.83) * 0.027 : 0;
    const gesture = !reduced ? Math.max(0, 1 - (t - this.gestureAt) / 1.6) : 0;
    const cheerful = /happy|joy|excited|greet|friendly|freud|fröh|begrü|freundlich/i.test(emotion);
    const thoughtful = /think|curious|nachdenk|neugier/i.test(emotion);
    r.head.rotation.x = idle ? Math.sin(t * 0.7) * 0.02 : 0;
    if (gesture > 0) r.head.rotation.x += Math.sin((t - this.gestureAt) * 7) * 0.07 * gesture;
    if (thoughtful) r.head.rotation.z += 0.08;
    r.arms[0].rotation.z = 0.04 + (idle ? Math.sin(t * 1.25) * 0.025 : 0);
    r.arms[1].rotation.z = -0.04 - (idle ? Math.sin(t * 1.25 + 0.8) * 0.025 : 0);
    if (cheerful && gesture > 0) r.arms[1].rotation.z -= Math.sin(gesture * Math.PI) * 1.25;
    r.arms[0].rotation.x = r.arms[1].rotation.x = idle ? Math.sin(t * 1.1) * 0.025 : 0;
    if (t > this.blinkAt + 0.16) this.blinkAt = t + 2.7 + Math.random() * 3.5;
    const blink = blinkEnabled && t >= this.blinkAt ? Math.sin((t - this.blinkAt) / 0.16 * Math.PI) : 0;
    for (const eye of r.eyes) {
      eye.scale.y = Math.max(0.055, (cheerful ? 0.82 : 1) * (1 - blink));
      eye.position.x = eye.userData.baseX + (idle ? Math.sin(t * 0.44) * 0.018 : 0);
    }
    r.mouth.scale.y = 1 + this.mouthLevel * 4;
    r.mouth.scale.x = 1 - this.mouthLevel * 0.15;
    r.jaw.position.y = -0.35 - this.mouthLevel * 0.045;
    (r.antenna.material as THREE.MeshStandardMaterial).emissiveIntensity = speaking ? 1 + this.mouthLevel * 0.8 : 0.5 + (idle ? Math.sin(t * 1.8) * 0.15 : 0);
    if (this.mixer && (idle || this.emote?.clip)) this.mixer.update(dt);
    this.animateEmote(reduced);
    this.animateSpeechGesture(reduced);
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
      this.lookAt.position.copy(this.camera.position);
      this.vrm.update(dt);
    }
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  };

  private clearImported() {
    this.finishEmote();
    this.mixer?.stopAllAction();
    if (this.mixer) this.mixer.uncacheRoot(this.mixer.getRoot());
    this.mixer = undefined;
    this.clips = [];
    this.idleAction = undefined;
    this.humanoidPose.clear();
    this.faceMeshes = [];
    this.faceVowels.clear();
    this.genericMouthRig = false;
    this.vrmExpressions.clear();
    this.mouthExpressions.clear();
    this.morphMotion = new WeakMap();
    this.genericHead = undefined;
    this.genericChest = undefined;
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
    ++this.request;
    cancelAnimationFrame(this.frame);
    this.observer.disconnect();
    this.controls.dispose();
    this.clearImported();
    disposeObject(this.scene);
    this.renderer.dispose();
    this.renderer.forceContextLoss();
    this.renderer.domElement.remove();
  }
}

function disposeObject(root: THREE.Object3D) {
  const geometries = new Set<THREE.BufferGeometry>();
  const materials = new Set<THREE.Material>();
  const textures = new Set<THREE.Texture>();
  const skeletons = new Set<THREE.Skeleton>();
  root.traverse((object) => {
    const mesh = object as THREE.Mesh;
    if (mesh.geometry) geometries.add(mesh.geometry);
    if (object instanceof THREE.SkinnedMesh) skeletons.add(object.skeleton);
    if (mesh.material) for (const material of Array.isArray(mesh.material) ? mesh.material : [mesh.material]) {
      materials.add(material);
      for (const value of Object.values(material)) if (value instanceof THREE.Texture) textures.add(value);
    }
  });
  textures.forEach((texture) => texture.dispose());
  materials.forEach((material) => material.dispose());
  geometries.forEach((geometry) => geometry.dispose());
  skeletons.forEach((skeleton) => skeleton.dispose());
}

function createRobot(): Robot {
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

  // Compact boots and articulated legs give Nova a grounded, toy-like silhouette.
  for (const side of [-1, 1]) {
    mesh(round(0.46, 0.23, 0.62, 0.1), porcelain, root, [side * 0.28, 0.13, 0.12]);
    mesh(round(0.39, 0.035, 0.5, 0.016), lavender, root, [side * 0.28, 0.045, 0.13]);
    mesh(new THREE.CylinderGeometry(0.14, 0.16, 0.4, 32), seams, root, [side * 0.26, 0.41, 0]);
    mesh(round(0.34, 0.37, 0.37, 0.12), ivory, root, [side * 0.26, 0.48, 0.02]);
    const knee = mesh(new THREE.CylinderGeometry(0.085, 0.085, 0.035, 32), accent, root, [side * 0.26, 0.52, 0.215]);
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
  return { root, head, torso, arms, eyes, mouth, jaw, antenna, accents: [accent, led] };
}
