import { titleFor } from './controls';

/**
 * The heading above a control.
 *
 * The screen used to show "Value" twice — humidity and temperature side by side — because it took the
 * slot name first and both slots are called `value`. These cases are the two shapes that exist in the
 * catalogue, in both directions.
 */
describe('titleFor', () => {
  it('names a reading by its capability, not by its generic slot', () => {
    expect(titleFor({ capability: 'humidity', slot: 'value', code: 'humidity.value' })).toBe(
      'Humidity',
    );
    expect(titleFor({ capability: 'temperature', slot: 'value', code: 'temperature.value' })).toBe(
      'Temperature',
    );
  });

  it('defers to the slot when the capability only names the kind of device', () => {
    // A bulb's capability is `switch`; "Switch" would be a label for the hardware, not for the thing
    // the control does.
    expect(titleFor({ capability: 'switch', slot: 'power', code: 'switch.power' })).toBe('Power');
    expect(titleFor({ capability: 'light', slot: 'brightness.level', code: 'brightness.level' })).toBe(
      'Level',
    );
  });

  it('falls back to the code when a product declares neither', () => {
    expect(titleFor({ code: 'custom.setpoint' })).toBe('Setpoint');
  });
});
