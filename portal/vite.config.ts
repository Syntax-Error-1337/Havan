import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react-swc';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    // The API is a separate origin in dev; proxying keeps the browser
    // same-origin, so the app's calls behave as they will in production.
    proxy: {
      '/v1': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
  // `vite preview` serves the built bundle and does not inherit `server.proxy`,
  // so without this the only way to check a build was to run the dev server —
  // which is not the same bundle.
  preview: {
    port: 4173,
    proxy: {
      '/v1': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
  build: { sourcemap: true },
});
