import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420, strictPort: true, host: '127.0.0.1',
    // Rust rebuilds must not reset the control room or its capture session.
    watch: { ignored: ['**/src-tauri/**', '**/vendor/**'] }
  },
  build: { target: 'es2022', sourcemap: true, chunkSizeWarningLimit: 1800 },
  envPrefix: ['VITE_', 'TAURI_ENV_']
});
