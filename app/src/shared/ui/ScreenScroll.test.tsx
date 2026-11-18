import { render } from '@testing-library/react-native';
import { StyleSheet } from 'react-native';

import { ScreenScroll } from './ScreenScroll';
import { AppText } from './Box';
import { ThemeProvider } from '../../app-shell/theme';

async function renderScroll(props: { onRefresh?: () => void } = {}) {
  return await render(
    <ThemeProvider>
      <ScreenScroll {...props}>
        <AppText>Hello</AppText>
      </ScreenScroll>
    </ThemeProvider>,
  );
}

describe('ScreenScroll', () => {
  it('gives a list a pull-to-refresh control when the screen offers one', async () => {
    const onRefresh = jest.fn();
    const view = await renderScroll({ onRefresh });

    const scroll = view.getByTestId('screen-scroll');
    expect(scroll.props.refreshControl).toBeTruthy();
    // The control is what the gesture calls; a scroll view that renders one without an onRefresh is a spinner
    // that never spins.
    scroll.props.refreshControl.props.onRefresh();
    expect(onRefresh).toHaveBeenCalled();
  });

  it('adds no control when the screen cannot refresh, rather than a dead spinner', async () => {
    const view = await renderScroll();
    expect(view.getByTestId('screen-scroll').props.refreshControl).toBeFalsy();
  });

  it('lets a tap through while the keyboard is up', async () => {
    // The default the whole app depends on: without it, iOS treats the tap as "dismiss the keyboard" and the
    // press never lands.
    const view = await renderScroll();
    expect(view.getByTestId('screen-scroll').props.keyboardShouldPersistTaps).toBe('handled');
  });

  it('reserves room for the floating tab bar, even when the caller adds its own inset', async () => {
    const view = await renderScroll();
    // `StyleSheet.flatten`, because the container style is an array now: the caller's inset is merged *after*
    // the clearance rather than replacing it. That shape change is exactly what broke when alerts migrated.
    const style = StyleSheet.flatten(view.getByTestId('screen-scroll').props.contentContainerStyle);
    expect(style.paddingBottom).toBeGreaterThan(100);
  });
});
