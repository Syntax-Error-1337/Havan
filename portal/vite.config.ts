import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react-swc';

export default defineConfig({
  plugins: [react()],
  server: {
    // Bind IPv4 explicitly: the default binding is `localhost`, which on macOS
    // resolves to ::1, so `http://127.0.0.1:5174` answered nothing at all while
    // `http://localhost:5174` worked. A dev URL that works on one name and not the
    // other wastes time in exactly the moment someone is checking a build.
    host: '127.0.0.1',
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
    host: '127.0.0.1',
    port: 4173,
    proxy: {
      '/v1': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
  build: { sourcemap: true },
});
