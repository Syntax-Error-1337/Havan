import { Component, type ErrorInfo, type ReactNode } from 'react';
import { View } from 'react-native';

import { AppText } from '../shared/ui/Box';
import { Button } from '../shared/ui/Button';
import { tokens } from '../shared/config/tokens';

/**
 * The last line of defence.
 *
 * A throw during render otherwise blanks the whole app — which is what happened
 * when a Bluetooth module loaded on web, where it does not exist: not a broken
 * radar screen, a white screen and a console message. This shows what failed and
 * offers the one action that can help, and it stays on for builds people use.
 *
 * It does not replace handling errors where they happen; it exists so a mistake in
 * one screen is a bad screen and not a dead app.
 */
export class ErrorBoundary extends Component<
  { children: ReactNode },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // No telemetry service yet, and a silent catch would be worse than the console.
    console.error('render failed', error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <View
        style={{
          flex: 1,
          padding: tokens.space.xl,
          gap: tokens.space.md,
          justifyContent: 'center',
          backgroundColor: '#000000',
        }}
      >
        <AppText variant="title" weight="600" style={{ color: '#FFFFFF' }}>
          Something went wrong
        </AppText>
        <AppText style={{ color: 'rgba(235,235,245,0.6)' }}>{error.message}</AppText>
        <Button label="Try again" onPress={() => this.setState({ error: null })} />
      </View>
    );
  }
}
