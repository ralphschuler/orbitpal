#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'public/decoders/manifest.json')));
const installed = JSON.parse(fs.readFileSync(path.join(root, 'node_modules/three/package.json'))).version;
if (installed !== manifest.threeVersion) throw new Error(`Review avatar decoders before using Three ${installed} (bundled ${manifest.threeVersion})`);
const hash = (file) => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
for (const file of manifest.files) {
  if (hash(path.join(root, 'node_modules/three', file.source)) !== (file.sourceSha256 || file.sha256)) throw new Error(`Three decoder source changed: ${file.source}`);
  if (file.path && hash(path.join(root, file.path)) !== file.sha256) throw new Error(`Bundled decoder changed: ${file.path}`);
}
for (const notice of manifest.notices) if (hash(path.join(root, notice.path)) !== notice.sha256) throw new Error(`Decoder license notice changed: ${notice.path}`);
const loader = fs.readFileSync(path.join(root, 'src/lib/avatar-loader.ts'), 'utf8');
if (!loader.includes(`/decoders/three-${installed}/`)) throw new Error('Avatar factory and reviewed decoder version differ');
console.log(`Verified ${manifest.files.length} local Three ${installed} decoder assets/modules and ${manifest.notices.length} notices.`);
