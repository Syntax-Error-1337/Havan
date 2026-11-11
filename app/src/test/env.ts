/**
 * Runs before any module is imported, so the radio factory in
 * `shared/lib/ble/index.ts` picks the stand-in instead of `react-native-ble-plx`
 * — which has no native module in the test runtime and would throw on import.
 */
process.env.EXPO_PUBLIC_BLE_MOCK = '1';
