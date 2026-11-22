import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { resolve } from 'node:path'
export default defineConfig({
  root: 'src/renderer',
  plugins: [svelte({ configFile: resolve('svelte.config.mjs') })],
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  build: { outDir: '../../out/renderer', emptyOutDir: true, sourcemap: false },
})
