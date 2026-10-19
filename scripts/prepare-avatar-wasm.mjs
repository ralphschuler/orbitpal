#!/usr/bin/env node
import fs from 'node:fs';import path from 'node:path';import {createHash} from 'node:crypto';import {fileURLToPath} from 'node:url';
import {capDecoderMemory} from './avatar-wasm-memory.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const file=path.join(root,'public/decoders/manifest.json');const manifest=JSON.parse(fs.readFileSync(file));
const hash=(bytes)=>createHash('sha256').update(bytes).digest('hex');
for(const asset of manifest.files.filter((asset)=>asset.path?.endsWith('.wasm'))) {
 const original=fs.readFileSync(path.join(root,'node_modules/three',asset.source));
 if(hash(original)!==(asset.sourceSha256||asset.sha256))throw new Error('Unreviewed decoder WASM source');
 const patched=capDecoderMemory(original);
 if(process.argv.includes('--write')) {
  fs.writeFileSync(path.join(root,asset.path),patched);asset.sourceSha256=hash(original);asset.sha256=hash(patched);asset.modification='WASM memory maximum changed from 2 GiB to 256 MiB; nonexecuting orbitpal.notice custom section added; scripts/prepare-avatar-wasm.mjs';
 } else if(hash(patched)!==asset.sha256||!patched.equals(fs.readFileSync(path.join(root,asset.path))))throw new Error('Decoder WASM memory cap is not reproducible');
}
if(process.argv.includes('--write'))fs.writeFileSync(file,JSON.stringify(manifest,null,2)+'\n');
console.log('Verified two decoder WASM memories capped at 256 MiB each.');
