import { useState } from 'react';
import { Pressable, ScrollView } from 'react-native';
import { useRouter } from 'expo-router';

import { Box, AppText } from '../../shared/ui';
import { useTheme, tokens } from '../../app-shell/theme';
import { outcome, useActivateScene, useScenes, type Scene } from '../../entities/scene';

/**
 * The home's scenes, as buttons.
 *
 * A scene is the fastest thing in the app: one press, several devices. So it
 * gets the top of the screen rather than a menu — and the result is shown in
 * place, because a press that silently does nothing is worse than one that fails
 * out loud.
 */
export function SceneStrip({
  homeId,
  scenes: provided,
  pending: waiting = false,
}: {
  homeId: string;
  /** The dashboard already fetched them; a second call for the same list is waste. */
  scenes?: Scene[];
  /** The owner is still loading. Render nothing rather than firing our own request. */
  pending?: boolean;
}) {
  const router = useRouter();
  const { color } = useTheme();
  // Only fetch when nobody else is feeding us: `provided` means the data is here,
  // `waiting` means it is on its way.
  const query = useScenes(homeId, provided === undefined && !waiting);
  const scenes = provided ?? query.data;
  const isPending = waiting || (provided === undefined && query.isPending);
  const activate = useActivateScene(homeId);
  const [note, setNote] = useState<{ sceneId: string; text: string; bad: boolean } | null>(null);

  // A home with no scenes shows no strip. It used to draw a dashed "+ Add a scene" card here, which was
  // the third place on one screen to create something — the centre button now offers that alongside "add a
  // device" and "add a room", and an empty strip above the device list is just a gap with a border.
  if (isPending) return null;
  if (!scenes || scenes.length === 0) return null;

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
        {/* The same height as a scene card, because a shorter control beside taller ones reads as the odd one out rather
            than as the way to add another. */}
        <Pressable
          onPress={() => router.push('/scenes')}
          accessibilityRole="button"
          accessibilityLabel="New scene"
        >
          <Box
            px="lg"
            py="md"
            radius="card"
            bg="surface"
            border
            style={{ minHeight: 64, minWidth: 96, justifyContent: 'center', alignItems: 'center' }}
          >
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
              {/* A scene chip holds two lines, and a stadium around two lines is a lozenge whose curves run through the
                  second one — measured on a phone at 66 pt tall, and worse at larger accessibility text sizes. A 22-pt
                  card radius holds them, and the row reads as cards rather than as one long control. */}
              <Box
                px="lg"
                py="md"
                radius="card"
                bg="elevated"
                border
                style={{ minHeight: 64, minWidth: 132, justifyContent: 'center', opacity: busy ? 0.6 : 1 }}
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
