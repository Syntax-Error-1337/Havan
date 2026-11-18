import { fireEvent, render } from '@testing-library/react-native';

import { RoomCapsule } from './RoomCapsule';
import { ThemeProvider } from '../../app-shell/theme';

/**
 * The room switcher.
 *
 * Two behaviours worth holding: the active room is a *state* a screen reader can hear (`role="tab"` +
 * `selected`), and a home with more rooms than a phone can hold hands the rest to a picker instead of growing
 * into an endless scroller. A reviewer watching a 1,200-device home called the scroller "cramped and endless" —
 * and that home had three rooms, so the fix had to be proved on a home that does not exist in the demo data.
 */

const rooms = (count: number) =>
  Array.from({ length: count }, (_, i) => ({ id: `r${i}`, name: `Room ${i}` }));

async function renderCapsule(props: Partial<React.ComponentProps<typeof RoomCapsule>> = {}) {
  return await render(
    <ThemeProvider>
      <RoomCapsule rooms={rooms(4)} value={null} onChange={() => undefined} {...props} />
    </ThemeProvider>,
  );
}

describe('room capsule', () => {
  it('marks the active room for assistive technology, not only with colour', async () => {
    const view = await renderCapsule({ value: 'r2' });

    const tab = view.getByLabelText('Room 2');
    expect(tab.props.accessibilityState).toEqual({ selected: true });
    expect(view.getByLabelText('Room 1').props.accessibilityState).toEqual({ selected: false });
  });

  it('says nothing at all for a home with one room', async () => {
    // A switcher with one option is a control that can only fail.
    const view = await renderCapsule({ rooms: rooms(1) });
    expect(view.queryByLabelText('Room 0')).toBeNull();
  });

  it('hands the overflow to a picker instead of growing the scroller', async () => {
    const seen: { id: string; name: string }[][] = [];
    const view = await renderCapsule({
      rooms: rooms(9),
      onMore: (all) => seen.push(all),
    });

    // Five visible rooms plus the "+4" affordance; the rest are behind it.
    expect(view.getByLabelText('Room 4')).toBeTruthy();
    expect(view.queryByLabelText('Room 5')).toBeNull();

    fireEvent.press(view.getByLabelText('+4'));
    expect(seen).toHaveLength(1);
    expect(seen[0]).toHaveLength(9);
  });

  it('keeps every room when there is no picker to hand them to', async () => {
    const view = await renderCapsule({ rooms: rooms(9) });
    expect(view.getByLabelText('Room 8')).toBeTruthy();
    expect(view.queryByLabelText('+4')).toBeNull();
  });
});
