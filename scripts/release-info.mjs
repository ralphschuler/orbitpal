import { appendFileSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export function releaseVersion(root = process.cwd()) {
  const packageVersion = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')).version;
  const tauriVersion = JSON.parse(readFileSync(resolve(root, 'src-tauri/tauri.conf.json'), 'utf8')).version;
  const cargo = readFileSync(resolve(root, 'src-tauri/Cargo.toml'), 'utf8');
  const packageSection = cargo.split(/^\[/m).find(section => section.startsWith('package]')) || '';
  const cargoVersion = packageSection.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const lock = readFileSync(resolve(root, 'src-tauri/Cargo.lock'), 'utf8');
  const lockVersion = lock.match(/\[\[package\]\]\r?\nname = "streamer-ai-cohost"\r?\nversion = "([^"]+)"/)?.[1];
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(packageVersion || '')) throw new Error('package.json needs a valid release version');
  for (const [file, version] of [['Cargo.toml', cargoVersion], ['tauri.conf.json', tauriVersion], ['Cargo.lock', lockVersion]]) {
    if (version !== packageVersion) throw new Error(`${file} version ${version ?? '(missing)'} differs from package.json ${packageVersion}`);
  }
  return packageVersion;
}

export function verifyTag(tag, version) {
  if (tag !== `v${version}`) throw new Error(`Release tag must be exactly v${version}`);
  return { tag, version, prerelease: version.includes('-') };
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const version = releaseVersion();
  const tag = process.argv[2];
  const result = tag ? verifyTag(tag, version) : { version };
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, Object.entries(result).map(([key, value]) => `${key}=${value}\n`).join(''));
  console.log(JSON.stringify(result));
}
