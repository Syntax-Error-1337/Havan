import { AppText, Row, Toggle } from '../../../shared/ui';
import { useSetPoint } from '../model/queries';
import { primaryPoint, type Device } from '../model/types';

/**
 * One device, as an iOS list row.
 *
 * It used to be a bordered tile with a long-press to toggle — a gesture nothing on
 * the screen advertised. A grouped row with a switch is the shape people already
 * know, and it works the same for a bulb and a plug without inventing a control.
 *
 * Only the primary point appears; the full capability set lives on the detail
 * screen, because a row that draws everything is a row nobody can scan.
 */
export function DeviceCard({ device, onPress }: { device: Device; onPress: () => void }) {
  const point = primaryPoint(device);
  const setPoint = useSetPoint(device.id);

  const isOn = point?.value === true || (typeof point?.value === 'number' && point.value > 0);

  const toggle = () => {
    if (!point?.writable) return;
    const next = typeof point.value === 'boolean' ? !point.value : isOn ? 0 : 100;
    setPoint.mutate({ code: point.code, value: next });
  };

  const subtitle = point
    ? `${point.code}${point.unit ? ` · ${point.unit}` : ''}${device.online ? '' : ' · offline'}`
    : device.category;

  return (
    <Row
      title={device.name}
      subtitle={subtitle}
      onPress={onPress}
      right={
        point?.writable ? (
          <Toggle
            value={isOn}
            onChange={toggle}
            // An offline device cannot act on a command; leaving the switch live
            // promises something the platform cannot deliver until it reconnects.
            disabled={!device.online || setPoint.isPending}
            label={`${device.name} ${isOn ? 'on' : 'off'}`}
          />
        ) : (
          <AppText variant="body" tone="muted">
            {point?.value === undefined || point?.value === null ? '—' : String(point.value)}
          </AppText>
        )
      }
    />
  );
}
