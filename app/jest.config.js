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
  setupFilesAfterEnv: ['<rootDir>/src/test/setup.ts'],
  transformIgnorePatterns: [
    'node_modules/(?!((jest-)?react-native|@react-native(-community)?)|expo(nent)?|@expo(nent)?/.*|@expo-google-fonts/.*|react-navigation|@react-navigation/.*|@sentry/react-native|native-base|react-native-svg)',
  ],
};
