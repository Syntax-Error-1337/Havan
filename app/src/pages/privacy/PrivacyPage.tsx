import { useState } from 'react';
import { ScrollView } from 'react-native';
import { useRouter } from 'expo-router';

import { AppText, Box, Button, Glass, Icon, Screen, ScreenHeader, useConfirm } from '../../shared/ui';
import { useTheme, tokens } from '../../app-shell/theme';
import { useSession } from '../../entities/session';
import { useHomes } from '../../entities/home';
import {
  describeErasure,
  describeExport,
  eraseAccount,
  fetchExport,
  saveExport,
} from '../../entities/privacy';

/**
 * The two rights a person has over their own data, in the product they live in.
 *
 * Both endpoints have existed since Phase 6. Neither had a screen, which is the difference between a
 * privacy policy and a privacy feature: a customer was legally owed a copy of their data and a way to
 * delete it, and the only way to get either was to ask somebody.
 */
export function PrivacyPage() {
  const router = useRouter();
  const { color } = useTheme();
  const signOut = useSession((s) => s.signOut);
  const { ask, sheet } = useConfirm();

  const [exporting, setExporting] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [erasing, setErasing] = useState(false);

  /**
   * The refusal the server would give, before the button is pressed.
   *
   * `erase` answers 409 when the person still owns a home that somebody else lives in — it will not
   * evict them or leave them with a home whose owner no longer exists. That is right, and it is also
   * the one refusal a real customer hits. A screen that lets them press the button to find out is a
   * screen that made them type a confirmation for nothing.
   */
  const homes = useHomes();
  // `Array.isArray`, because this line runs before the homes query resolves and a screen that crashes on
  // an unloaded list would take the export button with it.
  const list = Array.isArray(homes.data) ? homes.data : [];
  const sharedOwned = list.filter((h) => h.role === 'owner' && h.member_count > 1);
  const blocked = sharedOwned.length > 0;

  const download = async () => {
    setProblem(null);
    setSaved(null);
    setExporting(true);
    try {
      const bundle = await fetchExport();
      const where = await saveExport(bundle);
      // Saying *where* it went matters on a phone: the file is not in a Downloads folder the customer
      // can browse to, it is behind whatever the share sheet did with it.
      setSaved(`${where} · ${describeExport(bundle)}`);
    } catch {
      setProblem('Your data could not be prepared just now. Nothing was changed — please try again.');
    } finally {
      setExporting(false);
    }
  };

  const confirmErase = () => {
    ask({
      title: 'Delete your account?',
      message:
        'Your homes, devices and alerts are deleted and every session is signed out. A receipt of the request is kept for the retention window and then purged. This cannot be undone.',
      confirmLabel: 'Delete everything',
      cancelLabel: 'Keep my account',
      destructive: true,
      onConfirm: async () => {
        setProblem(null);
        setErasing(true);
        try {
          const report = await eraseAccount();
          // The receipt comes first, and signing out happens when it is acknowledged. Signing out
          // immediately unmounts this screen — the session gate redirects — and the person is left at a
          // login form wondering whether anything happened to the account they just deleted.
          ask({
            title: 'Your account is gone',
            message: describeErasure(report),
            confirmLabel: 'Close',
            cancelLabel: 'Close',
            onConfirm: () => {
              // The server has already revoked the refresh token; clearing the local session is what
              // stops the app from looking signed in to an account that no longer exists.
              void signOut();
              router.replace('/');
            },
          });
        } catch {
          setProblem('Your account could not be deleted. Nothing was changed — please try again.');
        } finally {
          setErasing(false);
        }
      },
    });
  };

  return (
    <Screen>
      {sheet}
      <ScreenHeader title="Privacy" onBack={() => router.back()} />
      <ScrollView
        contentContainerStyle={{
          padding: tokens.space.lg,
          gap: tokens.space.md,
          paddingBottom: tokens.layout.tabBarClearance,
        }}
      >
        <Glass>
          <Box gap="sm">
            <Box row gap="sm" style={{ alignItems: 'center' }}>
              <Icon name="check" size={16} color={color.accent} />
              <AppText weight="600">A copy of your data</AppText>
            </Box>
            <AppText variant="caption" tone="muted">
              Your account, your homes and devices, your alerts, and every reading your devices sent —
              as one JSON file. It also lists what is deliberately excluded, and why.
            </AppText>
            <Button
              label={exporting ? 'Preparing…' : 'Download a copy'}
              onPress={download}
              disabled={exporting}
              testID="export-data"
            />
            {saved ? (
              <AppText variant="tiny" tone="muted" testID="export-result">
                Saved {saved}
              </AppText>
            ) : null}
          </Box>
        </Glass>

        <Glass accent={color.danger}>
          <Box gap="sm">
            <Box row gap="sm" style={{ alignItems: 'center' }}>
              <Icon name="alert" size={16} color={color.danger} />
              <AppText weight="600">Delete your account</AppText>
            </Box>
            <AppText variant="caption" tone="muted">
              This cannot be undone. Every home you own is deleted, your devices are unpaired, your
              alerts are removed, and you are signed out everywhere. Homes you are a member of but do
              not own stay — only your membership is removed.
            </AppText>
            <AppText variant="caption" tone="muted">
              A receipt of the request is kept for the retention window and then purged. It records that
              the account was erased; it holds none of its readings.
            </AppText>
            {blocked ? (
              // Not a warning about a risk: a statement that the request will be refused, and what to
              // do instead. Without it the confirmation below is a form that cannot succeed.
              <AppText variant="caption" tone="danger" testID="erase-blocked">
                {sharedOwned.length === 1
                  ? `You still own ${sharedOwned[0]!.name}, and somebody else lives in it.`
                  : `You still own ${sharedOwned.length} homes that other people live in.`}{' '}
                Deleting your account would take their home away, so it is refused until the home is
                handed over or they have left it.
              </AppText>
            ) : null}
            <Button
              label={erasing ? 'Deleting…' : 'Delete my account'}
              variant="danger"
              onPress={confirmErase}
              disabled={erasing || blocked}
              testID="delete-account"
            />
            {blocked ? (
              <AppText variant="tiny" tone="muted">
                Nothing has been deleted. The button is off for the reason above, not because it failed.
              </AppText>
            ) : null}
          </Box>
        </Glass>

        {problem ? (
          <AppText variant="caption" tone="danger">
            {problem}
          </AppText>
        ) : null}
      </ScrollView>
    </Screen>
  );
}
