import { useMutation } from '@tanstack/react-query';

import { request } from '../../shared/api/client';
import { radio, type NearbyDevice } from '../../shared/lib/ble';

type Ticket = { ticket: string; expires_at: string };

/**
 * Pair one device, end to end: mint a ticket, hand it to the device over BLE
 * along with the Wi-Fi credentials, then wait for the device to claim itself.
 *
 * The claim is the device's job, not the phone's — it holds the secret and the
 * certificate comes back to it over its own connection. The phone only watches.
 */
export function usePairDevice(homeId: string) {
  return useMutation({
    mutationFn: async (input: {
      device: NearbyDevice;
      ssid: string;
      password: string;
      endpoint: string;
      timezone: string;
    }) => {
      const ticket = await request<Ticket>(`/homes/${homeId}/pairing-tickets`, {
        method: 'POST',
      });

      const result = await radio.provision({
        deviceId: input.device.id,
        ssid: input.ssid,
        password: input.password,
        ticket: ticket.ticket,
        endpoint: input.endpoint,
        timezone: input.timezone,
      });

      if (!result.ok) {
        throw new Error(result.message ?? 'The device did not accept the invitation.');
      }

      return { ticket: ticket.ticket, device: input.device };
    },
  });
}
