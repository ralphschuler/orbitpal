import { copyFileSync, existsSync, mkdirSync, readdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join, resolve, basename } from 'node:path';
import { pathToFileURL } from 'node:url';
import { releaseVersion } from './release-info.mjs';

export function platformForTarget(target) {
  const platforms = {
    'aarch64-apple-darwin': 'macos-arm64',
    'x86_64-apple-darwin': 'macos-x64',
    'x86_64-pc-windows-msvc': 'windows-x64',
    'x86_64-unknown-linux-gnu': 'linux-x64',
  };
  if (!platforms[target]) throw new Error(`Unsupported release target: ${target}`);
  return platforms[target];
}

function entries(root) {
  return readdirSync(root, { withFileTypes: true }).flatMap(entry => {
    const path = join(root, entry.name);
    if (entry.isSymbolicLink()) return [];
    if (entry.isDirectory() && !entry.name.endsWith('.app')) return entries(path);
    return [path];
  });
}

export function collect(root, target, output = resolve(root, 'release-artifacts')) {
  const platform = platformForTarget(target);
  const prefix = `Streamer-AI-Cohost-v${releaseVersion(root)}-${platform}`;
  const bundle = resolve(root, 'src-tauri/target', target, 'release/bundle');
  if (!existsSync(bundle)) throw new Error(`Bundle directory is missing: ${bundle}`);
  mkdirSync(output, { recursive: true });
  const built = entries(bundle);
  const copyOne = (extension, suffix = extension) => {
    const matches = built.filter(path => path.endsWith(extension));
    if (matches.length !== 1) throw new Error(`Expected exactly one ${extension} package for ${target}; found ${matches.length}`);
    const destination = join(output, `${prefix}${suffix}`);
    copyFileSync(matches[0], destination);
    return destination;
  };
  let result;
  if (platform.startsWith('macos-')) {
    const applications = built.filter(path => path.endsWith('.app'));
    if (applications.length !== 1) throw new Error(`Expected one macOS application; found ${applications.length}`);
    const archive = join(output, `${prefix}.zip`);
    execFileSync('/usr/bin/ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', applications[0], archive]);
    result = [archive, copyOne('.dmg'), copyOne('.app.tar.gz'), copyOne('.app.tar.gz.sig')];
  } else if (platform === 'windows-x64') {
    result = [copyOne('.exe', '-setup.exe'), copyOne('.exe.sig', '-setup.exe.sig'), copyOne('.msi'), copyOne('.msi.sig')];
  } else result = [copyOne('.AppImage'), copyOne('.AppImage.sig'), copyOne('.deb')];
  return result.map(path => basename(path));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  console.log(JSON.stringify(collect(process.cwd(), process.env.RUST_TARGET || process.argv[2])));
}
