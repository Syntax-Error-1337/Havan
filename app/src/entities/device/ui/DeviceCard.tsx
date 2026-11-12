import { AppText, Row, Toggle } from '../../../shared/ui';
import { commandNotice } from '../model/notices';
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
    // The kind decides the value, not the current one. A bool point that has never
    // reported a value has `value: null`, and inferring "off means 0" from that sent a
    // number to a bool point — the server refused it with "'switch.power' expects a bool
    // value", which is exactly what the app did the first time this row was tapped.
    const next = point.kind === 'bool' ? !isOn : isOn ? 0 : 100;
    setPoint.mutate({ code: point.code, value: next });
  };

  const label = point
    ? `${point.code}${point.unit ? ` · ${point.unit}` : ''}${device.online ? '' : ' · offline'}`
    : device.category;

  // A failed command used to flip the switch back in silence, which reads as a broken
  // switch. The reason goes where the row already has a second line, so nothing moves
  // and nothing has to be dismissed.
  const subtitle = setPoint.isError ? commandNotice(setPoint.error) : label;

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
