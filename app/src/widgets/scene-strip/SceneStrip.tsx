import { useState } from 'react';
import { Pressable, ScrollView } from 'react-native';
import { useRouter } from 'expo-router';

import { Box, AppText } from '../../shared/ui';
import { tokens } from '../../app-shell/theme';
import { outcome, useActivateScene, useScenes } from '../../entities/scene';

/**
 * The home's scenes, as buttons.
 *
 * A scene is the fastest thing in the app: one press, several devices. So it
 * gets the top of the screen rather than a menu — and the result is shown in
 * place, because a press that silently does nothing is worse than one that fails
 * out loud.
 */
export function SceneStrip({ homeId }: { homeId: string }) {
  const router = useRouter();
  const { data: scenes, isPending } = useScenes(homeId);
  const activate = useActivateScene(homeId);
  const [note, setNote] = useState<{ sceneId: string; text: string; bad: boolean } | null>(null);

  // With no scenes the strip still offers the way to make one, but only once
  // the home has devices: a scene with nothing to control is a dead end.
  if (isPending) return null;
  if (!scenes || scenes.length === 0) {
    return (
      <Box style={{ paddingHorizontal: tokens.space.lg }}>
        <Pressable onPress={() => router.push('/scenes')} accessibilityRole="button">
          <AppText variant="caption" tone="accent">
            + Add a scene
          </AppText>
        </Pressable>
      </Box>
    );
  }

  const run = async (sceneId: string) => {
    setNote(null);
    try {
      const result = await activate.mutateAsync(sceneId);
      setNote({ sceneId, text: outcome(result), bad: result.failed.length > 0 });
    } catch {
      setNote({ sceneId, text: 'Could not run that scene.', bad: true });
    }
  };

  return (
    <Box gap="xs">
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        contentContainerStyle={{ paddingHorizontal: tokens.space.lg, gap: tokens.space.sm }}
      >
        <Pressable onPress={() => router.push('/scenes')} accessibilityRole="button">
          <Box px="lg" py="md" radius="pill" bg="surface" border>
            <AppText variant="caption" tone="accent">
              + New
            </AppText>
          </Box>
        </Pressable>

        {scenes.map((scene) => {
          const busy = activate.isPending && activate.variables === scene.id;
          // A scene with no readable actions must not take the dashboard down
          // with it: the strip is a convenience above the device list.
          const actionCount = scene.actions?.length ?? 0;
          return (
            <Pressable
              key={scene.id}
              disabled={activate.isPending}
              onPress={() => void run(scene.id)}
              accessibilityRole="button"
              accessibilityLabel={`Run ${scene.name}`}
            >
              <Box
                px="lg"
                py="md"
                radius="pill"
                bg="elevated"
                border
                style={{ opacity: busy ? 0.6 : 1 }}
              >
                <AppText weight="600">{busy ? 'Running…' : scene.name}</AppText>
                <AppText variant="caption" tone="muted">
                  {`${actionCount} device${actionCount === 1 ? '' : 's'}`}
                </AppText>
              </Box>
            </Pressable>
          );
        })}
      </ScrollView>

      {note ? (
        <AppText
          variant="caption"
          tone={note.bad ? 'danger' : 'success'}
          style={{ paddingHorizontal: tokens.space.lg }}
        >
          {note.text}
        </AppText>
      ) : null}
    </Box>
  );
}
