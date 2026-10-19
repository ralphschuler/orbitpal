// Deterministic metadata for the bundled catalog. This reads bounded GLB JSON,
// never renders a model, fetches a URI or claims that a clip/audio was played.
// Native tests compare this generated data with avatar_metadata.rs inspection.
import { closeSync, openSync, readFileSync, readSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const files = {
  'builtin:robot-expressive': 'RobotExpressive.glb', 'builtin:fox': 'Fox.glb',
  'builtin:kaykit-knight': 'KayKitKnight.glb', 'builtin:kaykit-mage': 'KayKitMage.glb',
  'builtin:kaykit-rogue': 'KayKitRogue.glb', 'builtin:kaykit-skeleton': 'KayKitSkeleton.glb',
  'builtin:osa-mushy': 'osa/Mushy.vrm', 'builtin:osa-dracula': 'osa/Dracula.vrm',
  'builtin:osa-always-watching': 'osa/AlwaysWatching.vrm', 'builtin:osa-conehead-being': 'osa/ConeheadBeing.vrm',
  'builtin:osa-cool-alien': 'osa/CoolAlien.vrm', 'builtin:osa-cool-tiger': 'osa/CoolTiger.vrm',
  'builtin:osa-ghost-pumpking': 'osa/GhostPumpking.vrm', 'builtin:osa-max-hax': 'osa/MaxHax.vrm',
  'builtin:osa-megan-the-fox': 'osa/MeganTheFox.vrm', 'builtin:osa-paco': 'osa/Paco.vrm',
};
export const bundledAvatarFiles = files;

export function glbJson(path) {
  const descriptor = openSync(path, 'r');
  try {
    const header = Buffer.alloc(20);
    if (readSync(descriptor, header, 0, 20, 0) !== 20 || header.toString('ascii', 0, 4) !== 'glTF' || header.readUInt32LE(4) !== 2 || header.readUInt32LE(8) !== statSync(path).size || header.readUInt32LE(16) !== 0x4e4f534a) throw new Error(`Invalid bundled GLB: ${path}`);
    const size = header.readUInt32LE(12);
    if (!size || size > 8 * 1024 * 1024 || 20 + size > statSync(path).size) throw new Error(`Invalid bundled GLB JSON budget: ${path}`);
    const bytes = Buffer.alloc(size);
    if (readSync(descriptor, bytes, 0, size, 20) !== size) throw new Error(`Truncated bundled GLB JSON: ${path}`);
    return { document: JSON.parse(bytes.toString('utf8')), legacyPadding: size % 4 !== 0 };
  } finally { closeSync(descriptor); }
}
function vowel(name) {
  const key = name.toLowerCase().replace(/[^a-z0-9]/g, '').replace(/^(?:vrcv|viseme|mouth)/, '');
  return { a: 'aa', aa: 'aa', i: 'ih', ih: 'ih', u: 'ou', ou: 'ou', e: 'ee', ee: 'ee', o: 'oh', oh: 'oh' }[key];
}
const sorted = values => [...new Set(values)].sort();
function metadata({ document, legacyPadding }) {
  const legacy = document.extensions?.VRM;
  const modern = document.extensions?.VRMC_vrm;
  const vrm = legacy || modern;
  const bones = [], expressions = [], vowels = [];
  if (legacy) {
    bones.push(...(legacy.humanoid?.humanBones || []).map(bone => bone.bone));
    for (const group of legacy.blendShapeMaster?.blendShapeGroups || []) {
      const name = group.presetName && group.presetName !== 'unknown' ? group.presetName : group.name;
      if (name) expressions.push(name);
      if (group.binds?.length && vowel(name || '')) vowels.push(vowel(name));
    }
  } else if (modern) {
    bones.push(...Object.keys(modern.humanoid?.humanBones || {}));
    for (const kind of ['preset', 'custom']) for (const [name, expression] of Object.entries(modern.expressions?.[kind] || {})) {
      expressions.push(name);
      if (expression.morphTargetBinds?.length && vowel(name)) vowels.push(vowel(name));
    }
  } else {
    for (const skin of document.skins || []) for (const joint of skin.joints || []) bones.push(document.nodes?.[joint]?.name || `node_${joint}`);
    for (const mesh of document.meshes || []) for (const name of mesh.extras?.targetNames || []) {
      expressions.push(name); if (vowel(name)) vowels.push(vowel(name));
    }
  }
  const extensions = new Set([...(document.extensionsUsed || []), ...(document.extensionsRequired || [])]);
  const externalResources = [];
  function visit(value, path = '') {
    if (value && typeof value === 'object') for (const [name, child] of Object.entries(value)) {
      if (name === 'extras') continue;
      if (name === 'extensions' && child && typeof child === 'object') Object.keys(child).forEach(name => extensions.add(name));
      visit(child, `${path}/${name}`);
    }
  }
  visit(document);
  for (const kind of ['buffers', 'images']) (document[kind] || []).forEach((entry, index) => { if (typeof entry.uri === 'string' && !entry.uri.startsWith('data:')) externalResources.push(`${kind}/${index}/uri`); });
  const audioExtensions = sorted([...extensions].filter(name => /audio|sound|emitter/i.test(name)));
  if (externalResources.length || audioExtensions.length) throw new Error('Bundled asset needs a renderer/audio compatibility review before metadata generation');
  return {
    format: legacy ? 'vrm0' : modern ? 'vrm1' : 'glb2', version: vrm?.specVersion || '2.0', containerVersion: 2,
    animations: (document.animations || []).map((animation, index) => {
      const times = (animation.samplers || []).map(sampler => document.accessors?.[sampler.input]?.max?.[0]).filter(time => Number.isFinite(time) && time >= 0);
      return { name: animation.name || `animation_${index}`, index, duration: times.length ? Math.max(...times) : null };
    }),
    expressions: sorted(expressions), vowels: sorted(vowels), bones: sorted(bones), audioExtensions,
    requiredExtensions: document.extensionsRequired || [], usedExtensions: document.extensionsUsed || [], warnings: legacyPadding ? ['Eine ältere Dateiabweichung wird automatisch berücksichtigt.'] : [], externalResources,
  };
}
export function generateCapabilities(project = root) {
  const catalog = JSON.parse(readFileSync(join(project, 'public/character-catalog.json'), 'utf8'));
  const characters = {};
  for (const character of catalog) {
    if (!character.assetPath) {
      characters.nova = { format: 'builtin', version: 'procedural', containerVersion: null, animations: [], expressions: [], vowels: [], bones: [], audioExtensions: [], requiredExtensions: [], usedExtensions: [], warnings: [], externalResources: [] };
      continue;
    }
    if (!files[character.assetPath]) throw new Error(`Unmapped bundled character: ${character.assetPath}`);
    characters[character.assetPath] = metadata(glbJson(join(project, 'public/characters', files[character.assetPath])));
  }
  return { schemaVersion: 1, method: 'Declared metadata from bundled GLB JSON; not a rendering or audio playback test.', characters };
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const result = generateCapabilities();
  writeFileSync(join(root, 'public/character-capabilities.json'), JSON.stringify(result, null, 2) + '\n');
  console.log(`Generated metadata for ${Object.keys(result.characters).length} characters.`);
}
