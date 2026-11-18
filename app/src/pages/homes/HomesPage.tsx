import { useState } from 'react';
import { Pressable, ScrollView, TextInput, View } from 'react-native';
import { useRouter } from 'expo-router';

import { AppText, Button, Glass, Icon, Screen, ScreenHeader, ScreenScroll } from '../../shared/ui';
import { useCreateHome, useHomes } from '../../entities/home';
import { useSession } from '../../entities/session';
import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../../shared/config/tokens';

/**
 * Every home this account can open.
 *
 * The app assumed one home and stored its id: fine for the first year, wrong for the customer with a
 * flat and a cottage, and wrong for the operator who is looking at a bench home. Switching is also
 * how a large account is verified — a screen that only ever sees a six-device home never proves the
 * paged dashboard works.
 *
 * The device count comes from the server (`GET /homes`), not from a dashboard that has not been
 * loaded yet: a switcher that shows a dash for every home until you visit it is a switcher nobody
 * trusts.
 */
export function HomesPage() {
  const router = useRouter();
  const { color } = useTheme();
  const stored = useSession((s) => s.homeId);
  const setHome = useSession((s) => s.setHome);

  const { data: homes, isPending, isError, error } = useHomes();
  // The session holds the chosen home, and it is still empty until the dashboard picks one — so a person
  // who opens the switcher first sees three rows and no "Current" on any of them, which is exactly what the
  // screen is for. Until a choice is stored, the home the app would open is the first one.
  const current = stored ?? homes?.[0]?.id ?? null;
  // A chip that says the same word on every row is decoration. The role is shown only when the account
  // holds different roles in different homes — otherwise it is a fact about every row at once.
  const rolesDiffer = new Set((homes ?? []).map((h) => h.role)).size > 1;
  const createHome = useCreateHome();
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');
  const [timezone] = useState(
    () => Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC',
  );

  /**
   * Leaving after a choice.
   *
   * `router.back()` alone assumes this screen was pushed from the dashboard. It can also be the first
   * screen in the stack — a deep link, a reload on `/homes`, a notification — and then there is
   * nothing behind it and expo-router logs "GO_BACK was not handled". Ask, then leave either way.
   */
  const leave = () => {
    if (router.canGoBack()) router.back();
    else router.replace('/');
  };

  const choose = async (id: string) => {
    await setHome(id);
    leave();
  };

  return (
    <Screen>
      <ScreenHeader title="Homes" onBack={leave} />

      <ScreenScroll gap={tokens.space.md}>
        {isPending ? (
          <AppText variant="caption" tone="muted">
            Loading your homes…
          </AppText>
        ) : isError ? (
          // The server's own reason, not a shrug: a refused token and an empty account look the same
          // to a spinner and are completely different to a person.
          <AppText variant="caption" style={{ color: color.danger }}>
            {error.message}
          </AppText>
        ) : (
          (homes ?? []).map((home) => {
            const active = home.id === current;
            return (
              <Glass
                key={home.id}
                onPress={() => void choose(home.id)}
                accent={active ? color.accent : undefined}
                testID={`home-row-${home.id}`}
              >
                <View style={{ flexDirection: 'row', alignItems: 'center', gap: tokens.space.md }}>
                  {/* The home's initial, not one house glyph for every row: three identical houses are a
                      list you read, and the dashboard already uses an initial for the person. */}
                  <View
                    style={{
                      width: 34,
                      height: 34,
                      borderRadius: 17,
                      alignItems: 'center',
                      justifyContent: 'center',
                      backgroundColor: withAlpha(color.accent, active ? 0.24 : 0.12),
                    }}
                  >
                    <AppText variant="caption" weight="700" style={{ color: color.accent }}>
                      {(home.name.trim()[0] ?? 'H').toUpperCase()}
                    </AppText>
                  </View>
                  <View style={{ flex: 1, gap: 2 }}>
                    <View style={{ flexDirection: 'row', alignItems: 'center', gap: 6 }}>
                      <AppText variant="subhead" weight="600" numberOfLines={1} style={{ flexShrink: 1 }}>
                        {home.name}
                      </AppText>
                      {/* The role is a property of the row, so it sits with the name; the right-hand slot is
                          reserved for one thing — which home you are in. Mixing "Admin" and "Current" in the
                          same position made them look like the same kind of fact. */}
                      {rolesDiffer ? (
                        <View
                          style={{
                            paddingHorizontal: 6,
                            paddingVertical: 1,
                            borderRadius: 6,
                            backgroundColor: color.glass,
                          }}
                        >
                          <AppText variant="tiny" tone="muted" style={{ textTransform: 'capitalize' }}>
                            {home.role}
                          </AppText>
                        </View>
                      ) : null}
                    </View>
                    <AppText variant="tiny" tone="muted" numberOfLines={1}>
                      {home.device_count.toLocaleString()} device{home.device_count === 1 ? '' : 's'} ·{' '}
                      {home.member_count} {home.member_count === 1 ? 'member' : 'members'}
                    </AppText>
                  </View>
                  {active ? (
                    // A tick and the word, not a switch: a toggle in a list reads as a setting you
                    // can turn off, and this is a choice you are already in.
                    <View style={{ flexDirection: 'row', alignItems: 'center', gap: 5 }}>
                      <Icon name="check" size={14} color={color.accent} />
                      <AppText variant="tiny" style={{ color: color.accent }}>
                        Current
                      </AppText>
                    </View>
                  ) : null}
                </View>
              </Glass>
            );
          })
        )}

        <View style={{ gap: tokens.space.sm, marginTop: tokens.space.sm }}>
          {adding ? (
            <View style={{ flexDirection: 'row', gap: tokens.space.sm, alignItems: 'center' }}>
              <TextInput
                testID="new-home-name"
                placeholder="Home name"
                placeholderTextColor={color.muted}
                value={name}
                onChangeText={setName}
                autoFocus
                style={{
                  flex: 1,
                  backgroundColor: color.glass,
                  borderWidth: 1,
                  borderColor: color.border,
                  borderRadius: tokens.radius.md,
                  padding: tokens.space.md,
                  color: color.text,
                }}
              />
              <Button
                label={createHome.isPending ? 'Saving…' : 'Create'}
                disabled={name.trim().length === 0 || createHome.isPending}
                onPress={() =>
                  createHome.mutate(
                    { name: name.trim(), timezone },
                    {
                      onSuccess: (home) => {
                        setName('');
                        setAdding(false);
                        void choose(home.id);
                      },
                    },
                  )
                }
              />
            </View>
          ) : (
            <Pressable
              accessibilityRole="button"
              onPress={() => setAdding(true)}
              style={{
                flexDirection: 'row',
                alignItems: 'center',
                gap: 8,
                paddingHorizontal: 14,
                paddingVertical: 12,
                borderRadius: tokens.radius.lg,
                borderWidth: 1,
                borderStyle: 'dashed',
                borderColor: color.border,
              }}
            >
              <Icon name="plus" size={16} color={color.muted} />
              <AppText variant="caption" tone="muted">
                New home
              </AppText>
            </Pressable>
          )}

          {createHome.error ? (
            <AppText variant="caption" style={{ color: color.danger }}>
              {createHome.error.message}
            </AppText>
          ) : null}
        </View>
      </ScreenScroll>
    </Screen>
  );
}
