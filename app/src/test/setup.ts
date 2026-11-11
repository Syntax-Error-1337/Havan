/**
 * The pieces a screen touches that are native, or that would open a socket.
 * Each is mocked narrowly: a component that reaches for the network in a test
 * should fail, not silently connect.
 */

jest.mock('expo-secure-store', () => ({
  getItemAsync: jest.fn(async () => null),
  setItemAsync: jest.fn(async () => undefined),
  deleteItemAsync: jest.fn(async () => undefined),
}));

jest.mock('expo-router', () => ({
  useRouter: () => ({ push: jest.fn(), back: jest.fn(), replace: jest.fn() }),
  useLocalSearchParams: () => ({ id: 'device-1' }),
  Stack: ({ children }: { children?: unknown }) => children ?? null,
}));

jest.mock('react-native-safe-area-context', () => {
  const React = require('react');
  const { View } = require('react-native');
  return {
    SafeAreaView: ({ children, ...rest }: { children?: unknown }) =>
      React.createElement(View, rest, children),
    SafeAreaProvider: ({ children }: { children?: unknown }) => children,
    useSafeAreaInsets: () => ({ top: 0, right: 0, bottom: 0, left: 0 }),
  };
});

// No socket in tests: a stream that connects for real would make the suite
// depend on a running backend and fail on a train.
class DeadSocket {
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;
  send() {}
  close() {}
}
// @ts-expect-error — the test runtime has no WebSocket
global.WebSocket = DeadSocket;

afterEach(() => {
  jest.clearAllMocks();
});
