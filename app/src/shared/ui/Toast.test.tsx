import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { Pressable, Text } from 'react-native';

import { ThemeProvider } from '../../app-shell/theme';
import { ToastProvider, useToast } from './Toast';

/**
 * The toast is the app's only confirmation that a mutation happened, so the two properties that matter are worth a test
 * each: a second call replaces the first message (the last thing you did is what you want to hear about), and when a
 * message carries an action that action is pressable — a toast with an undo that cannot be pressed is a sentence with a
 * lie in it.
 */
function Trigger() {
  const toast = useToast();
  return (
    <Pressable testID="press" onPress={() => toast('Removed Light', 'done', { label: 'Undo', onPress: () => toast('Light is back') })}>
      <Text>press</Text>
    </Pressable>
  );
}

describe('Toast', () => {
  it('renders an action beside the message and calls it', async () => {
    const view = await render(
      <ThemeProvider>
        <ToastProvider>
          <Trigger />
        </ToastProvider>
      </ThemeProvider>,
    );

    await act(async () => {
      fireEvent.press(view.getByTestId('press'));
    });

    expect(view.getByText('Removed Light')).toBeTruthy();
    const undo = view.getByTestId('toast-action');
    expect(undo).toBeTruthy();

    await act(async () => {
      fireEvent.press(undo);
    });

    // A second toast replaces the first: the undo is only useful while it is the thing on screen.
    expect(view.getByText('Light is back')).toBeTruthy();
    expect(view.queryByText('Removed Light')).toBeNull();
    expect(screen.queryByTestId('toast-action')).toBeNull();
  });
});
