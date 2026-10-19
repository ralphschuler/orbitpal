import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { releaseVersion } from './release-info.mjs';

function publicSource(value) {
  if (typeof value !== 'string' || !/^(?:registry\+|git\+)?https?:\/\//.test(value)) return null;
  return value.replace(/(https?:\/\/)[^/@\s]+@/g, '$1').replace(/\?[^#]*/g, '');
}

export function rustInventory(metadata) {
  return (metadata.packages || []).map(pkg => ({
    name: pkg.name, version: pkg.version, license: pkg.license ?? null,
    license_file: pkg.license_file ? basename(pkg.license_file.replaceAll('\\', '/')) : null,
    repository: publicSource(pkg.repository), source: publicSource(pkg.source),
    authors: Array.isArray(pkg.authors) ? pkg.authors : [],
  })).sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`));
}

export function nodeInventory(groups) {
  return Object.entries(groups).flatMap(([license, entries]) => Array.isArray(entries) ? entries.map(pkg => ({
    name: pkg.name, versions: pkg.versions || (pkg.version ? [pkg.version] : []),
    license: pkg.license || license, homepage: publicSource(pkg.homepage),
  })) : []).sort((a, b) => a.name.localeCompare(b.name));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const platform = process.env.RELEASE_PLATFORM || 'linux-x64';
  if (platform !== 'linux-x64') throw new Error('Dependency inventories are generated once by the Linux release job');
  const options = { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 };
  const node = nodeInventory(JSON.parse(execFileSync('pnpm', ['licenses', 'list', '--json'], options)));
  const rust = rustInventory(JSON.parse(execFileSync('cargo', ['metadata', '--manifest-path', 'src-tauri/Cargo.toml', '--format-version', '1', '--locked'], options)));
  const output = resolve('release-artifacts');
  mkdirSync(output, { recursive: true });
  const prefix = `OrbitPal-v${releaseVersion()}-${platform}`;
  writeFileSync(resolve(output, `${prefix}-node-licenses.json`), `${JSON.stringify(node, null, 2)}\n`);
  writeFileSync(resolve(output, `${prefix}-rust-dependencies.json`), `${JSON.stringify(rust, null, 2)}\n`);
  console.log(`Dependency inventories: ${node.length} JavaScript entries, ${rust.length} Rust packages`);
}
