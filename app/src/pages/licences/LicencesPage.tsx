import { useMemo } from 'react';

import { useTheme } from '../../app-shell/theme';
import { Section, Row, Screen, ScreenHeader, ScreenScroll, AppText } from '../../shared/ui';
import { tokens } from '../../app-shell/theme';
import licences from '../../shared/config/licences.json';

/**
 * What is in this app, and under what licence.
 *
 * Every shipping app has to be able to answer that question — App Store review asks, an enterprise buyer asks first,
 * and this project's own rule is that only permissive licences ship (AGENTS.md §1). The list is generated from the
 * installed tree by `node scripts/gen-licences.mjs`, so it is a fact about the build rather than a page somebody
 * remembered to edit.
 */
export function LicencesPage() {
  const { color } = useTheme();

  const byLicense = useMemo(() => {
    const groups = new Map<string, string[]>();
    for (const pkg of licences.packages) {
      const list = groups.get(pkg.license) ?? [];
      list.push(`${pkg.name} ${pkg.version}`);
      groups.set(pkg.license, list);
    }
    return [...groups.entries()].sort((a, b) => b[1].length - a[1].length);
  }, []);

  return (
    <Screen>
      <ScreenHeader title="Licences" />
      <ScreenScroll gap={tokens.space.md}>
        <AppText variant="caption" tone="muted">
          Every package this app ships, by licence. Generated from the installed build on {licences.generated}; only
          permissive licences are allowed here.
        </AppText>

        {byLicense.map(([license, packages]) => (
          <Section key={license} title={`${license} · ${packages.length}`}>
            {packages.map((entry) => (
              <Row key={entry} title={entry} chevron={false} />
            ))}
          </Section>
        ))}

        <AppText variant="tiny" tone={color.muted ? 'muted' : undefined}>
          The Rust services have their own check: `scripts/licences.sh` fails the build on anything copyleft.
        </AppText>
      </ScreenScroll>
    </Screen>
  );
}
