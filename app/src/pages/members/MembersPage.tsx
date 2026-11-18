import { useState } from 'react';
import { Pressable, TextInput, View } from 'react-native';
import { useRouter } from 'expo-router';

import {
  AppText,
  Box,
  Button,
  Card,
  ChipPicker,
  QueryError,
  Row,
  Screen,
  ScreenHeader,
  ScreenScroll,
  Section,
  TouchText,
  useConfirm,
} from '../../shared/ui';
import {
  canManage,
  useCurrentHome,
  useInviteMember,
  useInvitations,
  useMembers,
  useRevokeInvitation,
  useRemoveMember,
  useSetMemberRole,
  type Member,
  type Role,
} from '../../entities/home';
import { useMe } from '../../entities/session';
import { useTheme, tokens } from '../../app-shell/theme';
import { useToast } from '../../shared/ui/Toast';

/**
 * The people in the home.
 *
 * The API has served this list, invitations, role changes and removals since the first version, and the app showed
 * one number — "2 members" — and a way to change neither. Inviting the person you live with is the feature a
 * smart-home app is bought for; a count is not.
 *
 * What this screen owes the customer is the limits, stated where they apply:
 *   the invitation adds an account we already know, because there is no mail transport to send a letter;
 *   only an admin can invite, re-role or remove somebody — and nobody can grant a role above their own;
 *   the owner is neither removable nor re-roled: ownership transfer is its own confirmed operation;
 *   leaving is always allowed, and it is your own row.
 */
export function MembersPage() {
  const router = useRouter();
  const { color } = useTheme();
  const { homeId, home } = useCurrentHome();
  const { data: me } = useMe();
  const { data: members, isPending, isError, refetch } = useMembers(homeId ?? undefined);

  const invite = useInviteMember();
  const changeRole = useSetMemberRole();
  const remove = useRemoveMember();
  const toast = useToast();
  const confirm = useConfirm();

  const [email, setEmail] = useState('');
  // The invitation link, shown exactly once: there is no mail transport, so the admin has to carry it to the person.
  const [inviteLink, setInviteLink] = useState<{ email: string; token: string; expiresAt: string } | null>(null);
  const [inviteRole, setInviteRole] = useState<Role>('member');
  const [editing, setEditing] = useState<string | null>(null);

  const mine = home?.role ?? 'guest';
  const mayManage = canManage(mine);
  const ownerId = home?.owner_id ?? members?.find((m) => m.role === 'owner')?.user_id ?? null;
  const listFailure = changeRole.error ?? remove.error ?? null;
  const inviteFailure = invite.error ? inviteWords(invite.error) : null;
  const { data: invites } = useInvitations(homeId ?? undefined);
  const revoke = useRevokeInvitation(homeId ?? undefined);

  return (
    <Screen>
      <ScreenHeader title="Members" />
      <ScreenScroll gap={tokens.space.lg}>
        {isError ? <QueryError onRetry={() => void refetch()} /> : null}

        <Section
          title="People"
          footnote="Guests can control devices. Members can also rename them. Admins can invite, remove and set roles."
        >
          {isPending ? (
            <Box p="md">
              <AppText tone="muted">Loading…</AppText>
            </Box>
          ) : (
            (members ?? []).map((member) => (
              <MemberRow
                key={member.user_id}
                member={member}
                isMe={member.user_id === me?.id}
                isOwner={member.user_id === ownerId}
                canEdit={mayManage && member.user_id !== ownerId && member.user_id !== me?.id}
                editing={editing === member.user_id}
                busy={changeRole.isPending || remove.isPending}
                onToggleEdit={() => setEditing(editing === member.user_id ? null : member.user_id)}
                onRole={(next) => {
                  if (!homeId) return;
                  changeRole.mutate(
                    { homeId, userId: member.user_id, role: next },
                    {
                      onSuccess: () => {
                        setEditing(null);
                        toast(`${member.display_name} is now ${next === 'admin' ? 'an admin' : `a ${next}`}`);
                      },
                    },
                  );
                }}
                onRemove={() => {
                  if (!homeId) return;
                  confirm.ask({
                    title: `Remove ${member.display_name}?`,
                    message: 'They lose access to this home and to every device in it.',
                    confirmLabel: 'Remove',
                    destructive: true,
                    onConfirm: () =>
                      remove.mutate(
                        { homeId, userId: member.user_id },
                        { onSuccess: () => toast(`${member.display_name} no longer has access`) },
                      ),
                  });
                }}
              />
            ))
          )}
          {listFailure ? (
            <Box px="md" py="sm">
              <AppText variant="tiny" tone="danger">
                {listFailure instanceof Error ? listFailure.message : 'That change was not saved.'}
              </AppText>
            </Box>
          ) : null}
        </Section>

        {mayManage ? (
          <Section
            title="Add someone"
            footnote="They need an account with this email. We cannot send invitations, so tell them yourself — they are in the home as soon as you add them."
          >
            <Box gap="sm" p="md">
              {/* A placeholder is not a label: it disappears the moment there is a value, and this field's value is
                  the whole point of the form. */}
              <AppText variant="tiny" tone="muted">
                Email
              </AppText>
              <TextInput
                value={email}
                onChangeText={setEmail}
                placeholder="name@example.com"
                placeholderTextColor={color.placeholder}
                autoCapitalize="none"
                autoCorrect={false}
                keyboardType="email-address"
                accessibilityLabel="Email address"
                style={{
                  backgroundColor: color.elevated,
                  borderWidth: 1,
                  borderColor: color.border,
                  borderRadius: tokens.radius.md,
                  padding: tokens.space.md,
                  color: color.text,
                  fontSize: tokens.font.body,
                }}
              />
              <ChipPicker
                label="Role"
                value={inviteRole}
                options={[
                  { id: 'guest', label: 'Guest' },
                  { id: 'member', label: 'Member' },
                  { id: 'admin', label: 'Admin' },
                ]}
                onChange={(next) => setInviteRole(next as Role)}
              />
              {(invites ?? []).length > 0 ? (
                <Box gap="xs">
                  <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.1 }}>
                    WAITING TO ACCEPT
                  </AppText>
                  {(invites ?? []).map((invite) => (
                    <Box key={invite.id} row gap="md" style={{ alignItems: 'center', minHeight: tokens.touch.min }}>
                      <Box flex={1} gap="xs">
                        <AppText numberOfLines={1}>{invite.email}</AppText>
                        <AppText variant="tiny" tone="muted">
                          {`Invited as ${invite.role}, expires ${new Date(invite.expires_at).toLocaleDateString()}`}
                        </AppText>
                      </Box>
                      <TouchText
                        label={`Revoke the invitation for ${invite.email}`}
                        onPress={() =>
                          revoke.mutate(invite.id, { onSuccess: () => toast('Invitation revoked') })
                        }
                      >
                        <AppText variant="caption" tone="danger">
                          Revoke
                        </AppText>
                      </TouchText>
                    </Box>
                  ))}
                </Box>
              ) : null}

              {inviteLink ? (
                <Card accent={color.accent} testID="invite-created">
                  <Box gap="xs">
                    <AppText weight="600">Invitation ready for {inviteLink.email}</AppText>
                    <AppText variant="caption" tone="muted">
                      Send them this link. It works once, for that address only, and stops working on{' '}
                      {new Date(inviteLink.expiresAt).toLocaleDateString()}.
                    </AppText>
                    <Box
                      style={{
                        paddingVertical: tokens.space.sm,
                        paddingHorizontal: tokens.space.md,
                        borderRadius: tokens.radius.md,
                        backgroundColor: color.elevated,
                        borderWidth: 1,
                        borderColor: color.border,
                      }}
                    >
                      <AppText variant="caption" selectable accessibilityLabel="Invitation link">
                        {`haven://join?token=${inviteLink.token}`}
                      </AppText>
                    </Box>
                    <Button label="Done" variant="ghost" onPress={() => setInviteLink(null)} />
                  </Box>
                </Card>
              ) : null}

              {inviteFailure ? (
                <AppText variant="tiny" tone="danger">
                  {inviteFailure}
                </AppText>
              ) : null}
              <Button
                label="Add to home"
                onPress={() => {
                  if (!homeId || !email.trim()) return;
                  invite.mutate(
                    { homeId, email: email.trim(), role: inviteRole },
                    {
                      onSuccess: (result) => {
                        setEmail('');
                        // "Added" and "invited" are different facts and need different next steps. The invitation case
                        // is the one that used to be a 404, and its link has to be seen now: there is no email.
                        // Only an explicit `invited` opens the link card: anything else — a 204 from an older server, a
                        // body a proxy stripped — means the person is in the home, which is what "added" has always
                        // meant. The other way round would show a link with no token in it.
                        if (result?.outcome !== 'invited') {
                          toast('Added to this home');
                          return;
                        }
                        setInviteLink({
                          email: email.trim(),
                          token: result.token,
                          expiresAt: result.expires_at,
                        });
                      },
                    },
                  );
                }}
                loading={invite.isPending}
                disabled={!email.trim()}
              />
            </Box>
          </Section>
        ) : null}

        {home && mine !== 'owner' ? (
          <Section title="Your membership" footnote="You can come back if somebody adds you again.">
            <Row
              title="Leave this home"
              destructive
              chevron={false}
              onPress={() => {
                if (!homeId || !me) return;
                confirm.ask({
                  title: `Leave ${home.name}?`,
                  message: 'You lose access to this home and to every device in it.',
                  confirmLabel: 'Leave',
                  destructive: true,
                  onConfirm: () =>
                    remove.mutate({ homeId, userId: me.id }, { onSuccess: () => toast('You left this home') }),
                });
              }}
            />
          </Section>
        ) : null}
      </ScreenScroll>
      {confirm.sheet}
    </Screen>
  );
}

function MemberRow({
  member,
  isMe,
  isOwner,
  canEdit,
  editing,
  busy,
  onToggleEdit,
  onRole,
  onRemove,
}: {
  member: Member;
  isMe: boolean;
  isOwner: boolean;
  canEdit: boolean;
  editing: boolean;
  busy: boolean;
  onToggleEdit: () => void;
  onRole: (role: Role) => void;
  onRemove: () => void;
}) {
  const { color } = useTheme();
  const initials = member.display_name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('');

  const words = isOwner ? 'Owner' : roleLabel(member.role);

  return (
    <Box gap="xs">
      <Box
        row
        gap="md"
        p="md"
        style={{ alignItems: 'center', borderTopWidth: 0.5, borderTopColor: color.border }}
      >
        {/* An avatar drawn from the name: no upload, no gravatar, and it reads as a person at a glance. */}
        <View
          style={{
            width: 40,
            height: 40,
            borderRadius: 20,
            alignItems: 'center',
            justifyContent: 'center',
            backgroundColor: color.glass,
            borderWidth: 0.5,
            borderColor: color.border,
          }}
        >
          <AppText weight="600" style={{ color: color.accentInk }}>
            {initials || '?'}
          </AppText>
        </View>
        <Box style={{ flex: 1 }} gap="xs">
          <AppText weight="600">
            {member.display_name}
            {isMe ? ' (you)' : ''}
          </AppText>
          {member.email ? (
            <AppText variant="tiny" tone="muted">
              {member.email}
            </AppText>
          ) : null}
        </Box>
        {canEdit ? (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`Role for ${member.display_name}: ${words}`}
            accessibilityHint="Opens the role chooser"
            onPress={onToggleEdit}
            style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
          >
            <AppText tone="accent">{words}</AppText>
          </Pressable>
        ) : (
          <AppText tone="muted">{words}</AppText>
        )}
      </Box>

      {editing && canEdit ? (
        // The role chooser and the removal live under the same row rather than in a modal: one person is being
        // edited, and the screen is short enough that the context stays on screen.
        <Box gap="sm" px="md" py="sm">
          <ChipPicker
            label={`Role for ${member.display_name}`}
            value={member.role}
            options={[
              { id: 'guest', label: 'Guest' },
              { id: 'member', label: 'Member' },
              { id: 'admin', label: 'Admin' },
            ]}
            onChange={(next) => onRole(next as Role)}
          />
          <Pressable
            accessibilityRole="button"
            onPress={onRemove}
            disabled={busy}
            style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
          >
            <AppText tone="danger">
              {busy ? 'Working…' : `Remove ${member.display_name} from this home`}
            </AppText>
          </Pressable>
        </Box>
      ) : null}
    </Box>
  );
}

/**
 * The server says "account with that email not found"; that is an answer, not a sentence.
 *
 * A person reading it needs to know what to do next, and there is exactly one next step: check the spelling, or ask
 * them to sign up first. Anything else is passed through unchanged, because a message we rewrite is a message we can
 * get wrong.
 */
export function inviteWords(error: unknown): string {
  const text = error instanceof Error ? error.message : '';
  if (/not found/i.test(text)) {
    return 'Nobody in Haven has that email. Check the spelling, or ask them to create an account first.';
  }
  return text || 'That person could not be added.';
}

function roleLabel(role: Role): string {
  switch (role) {
    case 'guest':
      return 'Guest';
    case 'member':
      return 'Member';
    case 'admin':
      return 'Admin';
    case 'owner':
      return 'Owner';
    default:
      return role;
  }
}
