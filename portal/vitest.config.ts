import { defineConfig } from 'vitest/config';

/**
 * The portal had no tests, and a green `vite build` says only that the types
 * line up. These render the pages: the vendor's first minute (sign in) and the
 * document they spend their week in (the thing-model editor).
 *
 * No React plugin here on purpose: vitest compiles TSX through esbuild using the
 * tsconfig's `jsx: react-jsx`, and the SWC plugin's types clash with vitest's
 * bundled vite.
 */
export default defineConfig({
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.tsx'],
  },
});
