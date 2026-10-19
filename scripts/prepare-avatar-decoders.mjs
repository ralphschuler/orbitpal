#!/usr/bin/env node
// Reproducible, offline vendoring from the installed locked Three package.
// Precompile its fixed Basis WASM's Embind invokers rather than granting eval
// to the application. No model, image or user code is loaded by this script.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const three = path.join(root, 'node_modules/three');
const manifestPath = path.join(root, 'public/decoders/manifest.json');
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const manifest = JSON.parse(fs.readFileSync(manifestPath));
const installed = JSON.parse(fs.readFileSync(path.join(three, 'package.json'))).version;
if (installed !== manifest.threeVersion) throw new Error(`Decoder review required: installed Three ${installed}, reviewed ${manifest.threeVersion}`);
const basis = manifest.files.find((file) => file.source.endsWith('/basis_transcoder.js'));
const bytes = fs.readFileSync(path.join(three, basis.source));
if (hash(bytes) !== (basis.sourceSha256 || basis.sha256)) throw new Error('Unreviewed Basis wrapper source hash');
const wasm = manifest.files.find((file) => file.source.endsWith('/basis_transcoder.wasm'));
const wasmBytes = fs.readFileSync(path.join(three, wasm.source));
if (hash(wasmBytes) !== (wasm.sourceSha256 || wasm.sha256)) throw new Error('Unreviewed Basis WASM source hash');
const signatures = new Map();
const scope = vm.createContext({ console, WebAssembly, TextDecoder, TextEncoder, Uint8Array, ArrayBuffer, setTimeout, clearTimeout, performance, process, require: createRequire(import.meta.url), __dirname: three, __filename: 'basis_transcoder.js' });
const OriginalFunction = vm.runInContext('Function', scope);
scope.Function = new Proxy(OriginalFunction, { apply(target, receiver, args) { signatures.set(JSON.stringify(args), args); return Reflect.apply(target, receiver, args); } });
vm.runInContext(bytes.toString('utf8'), scope);
const module = await scope.BASIS({ wasmBinary: wasmBytes });
module.initializeBasis();
// Emval creates additional fixed method-callers lazily at the first image.
const image = new module.KTX2File(new Uint8Array(fs.readFileSync(path.join(root, 'tests/fixtures/compressed-avatar/red-uastc.ktx2'))));
if (!image.isValid() || !image.startTranscoding()) throw new Error('Original UASTC fixture failed');
const pixels = new Uint8Array(image.getImageTranscodedSizeInBytes(0, 0, 0, 13));
if (!image.transcodeImage(pixels, 0, 0, 0, 13, 0, -1, -1)) throw new Error('Original UASTC fixture did not transcode');
image.close(); image.delete();
if (signatures.size === 0 || signatures.size > 256) throw new Error('Unexpected Embind signature inventory');
const needle = 'function newFunc(constructor,argumentList){';
let patched = bytes.toString('utf8');
if (patched.split(needle).length !== 2) throw new Error('Basis wrapper changed: review the CSP patch');
const table = [...signatures].map(([key, args]) => `${JSON.stringify(key)}:function(${args.slice(0, -1).join(',')}){${args.at(-1)}}`).join(',\n');
const helper = `\n// OrbitPal modification: fixed Embind invokers precompiled offline from this\n// exact WASM. No Function/eval or user-provided code is executed at runtime.\nfunction orbitpalStaticInvoker(args){var fn=orbitpalStaticInvokers[JSON.stringify(args)];if(!fn)throw new Error("Unreviewed Basis Embind signature");return fn;}\nvar orbitpalStaticInvokers={${table}};\n`;
// Helpers must live inside the module factory, where registration invokes them.
patched = patched.replace('var Module=moduleArg;', helper + 'var Module=moduleArg;').replace(needle, needle + 'if(constructor===Function)return orbitpalStaticInvoker(argumentList);');
const notice = '// OrbitPal: modified Basis JS wrapper; statically precompiled Embind invokers.\n// See public/decoders/README.md and manifest.json for source, license and hashes.\n';
const output = Buffer.from(notice + patched);
if (process.argv.includes('--write')) {
  fs.writeFileSync(path.join(root, basis.path), output);
  basis.sourceSha256 = hash(bytes); basis.sha256 = hash(output); basis.modification = 'Static Embind invokers; scripts/prepare-avatar-decoders.mjs (no runtime eval)';
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
  console.log(`Prepared ${signatures.size} fixed Basis invoker signatures.`);
} else {
  if (hash(output) !== basis.sha256 || !output.equals(fs.readFileSync(path.join(root, basis.path)))) throw new Error('Basis CSP patch is not reproducible');
  console.log(`Verified ${signatures.size} fixed Basis invoker signatures.`);
}
