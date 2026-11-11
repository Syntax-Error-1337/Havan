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

/**
 * No socket in tests: a stream that connects for real would make the suite
 * depend on a running backend. This records what the code under test did, so a
 * test can drive it: `lastSocket().open()`, `.emit({...})`.
 */
class TestSocket {
  static instances: TestSocket[] = [];

  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;

  sent: string[] = [];
  closed = false;

  constructor(readonly url: string) {
    TestSocket.instances.push(this);
  }

  send(data: string) {
    this.sent.push(data);
  }

  close() {
    this.closed = true;
  }

  open() {
    this.onopen?.();
  }

  emit(message: unknown) {
    this.onmessage?.({ data: JSON.stringify(message) });
  }

  emitRaw(data: string) {
    this.onmessage?.({ data });
  }
}

(globalThis as unknown as { WebSocket: unknown }).WebSocket = TestSocket;
(globalThis as unknown as { __lastSocket: () => TestSocket | undefined }).__lastSocket = () =>
  TestSocket.instances[TestSocket.instances.length - 1];
// Sockets accumulate across tests; start each one from a clean slate so a test
// cannot emit into a connection that was closed by the previous test.
(globalThis as unknown as { __resetSockets: () => void }).__resetSockets = () => {
  TestSocket.instances = [];
};

afterEach(() => {
  jest.clearAllMocks();
});
