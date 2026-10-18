#!/usr/bin/env node
// Collect published license/copyright notices without executing package code.
// Run after pnpm install and cargo fetch, before the frontend/native build.
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const destination = path.join(root, 'public/licenses/DEPENDENCIES.txt');
const warnings = [];
const MAX_TEXT = 2 * 1024 * 1024;
const replacements = [
  [root, '<project>'],
  ...(process.env.CARGO_HOME ? [[process.env.CARGO_HOME, '<cargo-cache>']] : []),
  [os.homedir(), '<user-home>'],
].sort((a, b) => b[0].length - a[0].length);
function clean(value) {
  let text = String(value);
  for (const [local, replacement] of replacements) {
    if (local.length > 1) text = text.replaceAll(local, replacement).replaceAll(local.replaceAll('\\', '/'), replacement);
  }
  return text;
}
function warn(pkg, message) { warnings.push(`${pkg}: ${message}`); }
function json(file) { return JSON.parse(fs.readFileSync(file, 'utf8')); }
function person(value) {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object') return [value.name, value.email ? `<${value.email}>` : '', value.url].filter(Boolean).join(' ');
  return '';
}
function spdx(value) {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object') return value.type || 'UNKNOWN';
  return 'UNKNOWN';
}
function within(parent, child) {
  const relative = path.relative(parent, child);
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
}
const noticeName = /^(?:unlicen[cs]e|licen[cs](?:e|ing)|notice|copying|copyright|authors)(?:[._-].*)?$/i;
const licenseName = /^(?:unlicen[cs]e|licen[cs](?:e|ing)|copying)(?:[._-].*)?$/i;
const noticeDirectory = /^(?:licen[cs]es?|notices?)$/i;
function notices(directory, label, explicitFile) {
  const found = new Map();
  let visited = 0;
  function add(file, isLicense) {
    if (found.has(file)) { if (isLicense) found.get(file).isLicense = true; return; }
    const relative = path.relative(directory, file).replaceAll(path.sep, '/');
    const stat = fs.lstatSync(file);
    if (!stat.isFile() || stat.isSymbolicLink()) return;
    if (stat.size > MAX_TEXT) { warn(label, `notice ${relative} exceeds 2 MiB and was not copied`); return; }
    const bytes = fs.readFileSync(file);
    if (bytes.includes(0)) { warn(label, `notice ${relative} is binary and was not copied`); return; }
    found.set(file, { relative, text: bytes.toString('utf8'), isLicense });
  }
  function visit(current, depth, inNotices = false) {
    if (depth > 5) return;
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      if (++visited > 20_000) return;
      if (entry.isSymbolicLink() || ['node_modules', '.git', 'target', '.cache'].includes(entry.name)) continue;
      const file = path.join(current, entry.name);
      if (entry.isDirectory()) visit(file, depth + 1, inNotices || noticeDirectory.test(entry.name));
      else if (entry.isFile() && (inNotices || noticeName.test(entry.name))) add(file, inNotices || licenseName.test(entry.name));
    }
  }
  visit(directory, 0);
  if (visited > 20_000) warn(label, 'source-tree notice scan reached its file-count limit');
  if (explicitFile) {
    const file = path.resolve(directory, explicitFile);
    if (!within(directory, file)) warn(label, 'declared license_file is outside the packaged source and was not read');
    else if (fs.existsSync(file)) add(file, true);
    else warn(label, 'declared license_file is missing from the installed source');
  }
  if (![...found.values()].some(file => file.isLicense && file.text.trim())) warn(label, 'NO STANDALONE LICENSE TEXT FOUND; inspect the upstream distribution before redistribution');
  return [...found.values()].sort((a, b) => a.relative.localeCompare(b.relative));
}

const installed = new Map();
const seenDirectories = new Set();
function packageDirectory(directory) {
  let real;
  try { real = fs.realpathSync(directory); } catch { return; }
  if (seenDirectories.has(real)) return;
  seenDirectories.add(real);
  const manifest = path.join(real, 'package.json');
  if (!fs.existsSync(manifest)) return;
  const data = json(manifest);
  if (data.name && data.version) {
    const id = `${data.name}@${data.version}`;
    if (!installed.has(id)) installed.set(id, { data, directory: real });
  }
  modulesDirectory(path.join(real, 'node_modules'));
}
function modulesDirectory(directory) {
  if (!fs.existsSync(directory)) return;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === '.pnpm') {
      for (const entry of fs.readdirSync(path.join(directory, '.pnpm'), { withFileTypes: true })) {
        if (entry.isDirectory()) modulesDirectory(path.join(directory, '.pnpm', entry.name, 'node_modules'));
      }
    } else if (entry.name.startsWith('.')) continue;
    else if (entry.name.startsWith('@')) {
      for (const child of fs.readdirSync(path.join(directory, entry.name))) packageDirectory(path.join(directory, entry.name, child));
    } else packageDirectory(path.join(directory, entry.name));
  }
}
if (!fs.existsSync(path.join(root, 'node_modules'))) throw new Error('Run pnpm install --frozen-lockfile before collecting dependency notices.');
modulesDirectory(path.join(root, 'node_modules'));
if (!installed.size) throw new Error('No installed JavaScript packages were found.');

const cargo = spawnSync(process.env.CARGO || 'cargo', ['metadata', '--locked', '--offline', '--format-version', '1', '--manifest-path', path.join(root, 'src-tauri/Cargo.toml')], {
  cwd: root, env: process.env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 120_000,
});
if (cargo.error || cargo.status !== 0) throw new Error('Cargo metadata could not be read. Install Rust, run cargo fetch --locked, and ensure cargo is on PATH (or set CARGO). No raw metadata or diagnostic paths were written.');
const metadata = JSON.parse(cargo.stdout);
const members = new Set(metadata.workspace_members);
const rust = metadata.packages.filter(pkg => !members.has(pkg.id)).sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`));

const sections = [
  'Streamer AI Cohost — dependency license and copyright notices',
  '============================================================',
  '',
  'Generated from installed JavaScript packages and Cargo.lock-resolved Rust package metadata.',
  'Includes development/build and target-specific dependencies when their source is installed.',
  'Original license/copyright/NOTICE/AUTHORS files are reproduced below; package metadata is informational.',
  'This collection does not relicense any dependency. Optional missing-text warnings appear at the end.',
  'Character-model and separately bundled font notices are supplied alongside this file.',
  '',
  `JavaScript packages: ${installed.size}; Rust packages: ${rust.length}`,
  '',
  'PROJECT LICENSE',
  '===============',
  fs.readFileSync(path.join(root, 'LICENSE'), 'utf8').trim(),
];
function section(kind, name, version, license, authors, files) {
  const label = `${kind}: ${name}@${version}`;
  sections.push('', '='.repeat(78), label, `License metadata: ${license || 'UNKNOWN'}`);
  if (authors.length) sections.push(`Authors / contributors from published metadata: ${[...new Set(authors.filter(Boolean))].join('; ')}`);
  for (const file of files) sections.push('', `--- ${file.relative} ---`, file.text.trimEnd());
}
for (const [id, { data, directory }] of [...installed.entries()].sort(([a], [b]) => a.localeCompare(b))) {
  const authors = [person(data.author), ...(Array.isArray(data.contributors) ? data.contributors.map(person) : [])];
  const license = data.license ? spdx(data.license) : Array.isArray(data.licenses) ? data.licenses.map(spdx).join(' OR ') : 'UNKNOWN';
  section('JavaScript', data.name, data.version, license, authors, notices(directory, id));
}
for (const pkg of rust) {
  const directory = path.dirname(pkg.manifest_path);
  section('Rust', pkg.name, pkg.version, pkg.license, pkg.authors || [], notices(directory, `${pkg.name}@${pkg.version}`, pkg.license_file));
}
sections.push('', 'COLLECTION WARNINGS', '===================', ...(warnings.length ? warnings : ['None.']), '');
fs.mkdirSync(path.dirname(destination), { recursive: true });
fs.writeFileSync(destination, clean(sections.join('\n')));
console.log(`Wrote public/licenses/DEPENDENCIES.txt: ${installed.size} JavaScript and ${rust.length} Rust packages; ${warnings.length} warning(s).`);
for (const warning of warnings) console.warn(`License notice warning: ${clean(warning)}`);
if (process.argv.includes('--strict') && warnings.length) process.exitCode = 1;
