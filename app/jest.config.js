/**
 * Component tests, in the app's own runtime.
 *
 * There is no simulator on this machine and no Xcode, so "does the screen work"
 * was answered by typecheck alone — which is how `useDevices` shipped reading
 * `{data}` from a bare array and rendered "no devices" for every home. These
 * tests render the real screens against a stubbed network and assert what a
 * person would see.
 */
module.exports = {
  preset: 'jest-expo',
  testMatch: ['**/src/**/*.test.tsx', '**/src/**/*.test.ts'],
  // Before any import: the radio is chosen at import time.
  setupFiles: ['<rootDir>/src/test/env.ts'],
  // A cold runner — fresh `npm ci`, empty transform cache — is several times
  // slower than a warm laptop, and the default 5 s turned six passing tests into
  // timeouts on exactly that run. The limit is for "this test hung", not "this
  // machine is cold".
  testTimeout: 15_000,
  setupFilesAfterEnv: ['<rootDir>/src/test/setup.ts'],
  transformIgnorePatterns: [
    'node_modules/(?!((jest-)?react-native|@react-native(-community)?)|expo(nent)?|@expo(nent)?/.*|@expo-google-fonts/.*|react-navigation|@react-navigation/.*|@sentry/react-native|native-base|react-native-svg)',
  ],
};
