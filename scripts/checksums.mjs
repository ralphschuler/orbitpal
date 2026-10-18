import { createHash } from 'node:crypto';
import { createReadStream, lstatSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';

const directory = resolve(process.argv[2] || 'release-artifacts');
const files = readdirSync(directory).filter(name => name !== 'SHA256SUMS').sort();
if (!files.length) throw new Error('No release assets to checksum');
const lines = [];
for (const name of files) {
  const path = join(directory, name);
  const info = lstatSync(path);
  if (!info.isFile() || info.isSymbolicLink() || /[\r\n]/.test(name)) throw new Error(`Unexpected release asset: ${name}`);
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  lines.push(`${hash.digest('hex')}  ${name}\n`);
}
writeFileSync(join(directory, 'SHA256SUMS'), lines.join(''));
console.log(`SHA256SUMS contains ${lines.length} assets`);
