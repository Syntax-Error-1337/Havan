import { useState } from 'react';
import { Pressable, ScrollView, TextInput, View } from 'react-native';
import { useRouter } from 'expo-router';

import { AppText, Button, Glass, Icon, Screen } from '../../shared/ui';
import { useCreateHome, useHomes } from '../../entities/home';
import { useSession } from '../../entities/session';
import { useTheme, tokens } from '../../app-shell/theme';

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
  const current = useSession((s) => s.homeId);
  const setHome = useSession((s) => s.setHome);

  const { data: homes, isPending, isError, error } = useHomes();
  const createHome = useCreateHome();
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');
  const [timezone] = useState(
    () => Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC',
  );

  const choose = async (id: string) => {
    await setHome(id);
    router.back();
  };

  return (
    <Screen>
      <View
        style={{
          flexDirection: 'row',
          alignItems: 'center',
          gap: tokens.space.sm,
          paddingHorizontal: tokens.space.lg,
          paddingVertical: tokens.space.md,
        }}
      >
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Back"
          hitSlop={12}
          onPress={() => router.back()}
        >
          <View style={{ transform: [{ rotate: '180deg' }] }}>
            <Icon name="chevron-right" size={20} color={color.muted} />
          </View>
        </Pressable>
        <AppText variant="title" weight="700">
          Homes
        </AppText>
      </View>

      <ScrollView contentContainerStyle={{ padding: tokens.space.lg, gap: tokens.space.md }}>
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
                  <View style={{ flex: 1, gap: 2 }}>
                    <AppText variant="subhead" weight="600" numberOfLines={1}>
                      {home.name}
                    </AppText>
                    <AppText variant="tiny" tone="muted">
                      {home.device_count} device{home.device_count === 1 ? '' : 's'} ·{' '}
                      {home.member_count} {home.member_count === 1 ? 'person' : 'people'} ·{' '}
                      {home.role}
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
      </ScrollView>
    </Screen>
  );
}
