import { build } from 'vite';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../../', import.meta.url));
await build({
  configFile: false,
  root,
  base: './',
  publicDir: false,
  mode: 'production',
  build: {
    outDir: resolve(root, 'dist-benchmark'),
    emptyOutDir: true,
    target: 'es2022',
    sourcemap: false,
    rollupOptions: { input: resolve(root, 'benchmark.html') },
  },
});
