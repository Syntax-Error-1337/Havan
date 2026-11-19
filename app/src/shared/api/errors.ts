/**
 * What a customer reads when a screen does not load.
 *
 * The app had the mirror image of the portal's problem. The portal printed the raw error (`TypeError: Failed to fetch`);
 * the app printed one fixed sentence — "the app could not reach the hub" — even when the hub had answered perfectly well
 * and said something specific. A person told they are offline when in fact the home was deleted will go hunting for Wi-Fi.
 *
 * So the mapping distinguishes three cases that used to look identical:
 *   * the platform answered and its sentence is meant for a person — repeat it;
 *   * the platform answered with something generic — say what the status means for *this* screen;
 *   * nothing came back — say the connection failed, because that is the one case where "nothing has changed" is certain.
 */
import { ApiError } from './client';

export type FailureCopy = {
  /** The sentence to show under the title. */
  body: string;
  /** True when trying again cannot help, so the retry control should not be offered. */
  permanent: boolean;
};

const GENERIC = /^request failed \(\d+\)$/i;

export function describeFailure(error: unknown): FailureCopy {
  if (error instanceof ApiError) {
    const said = error.message?.trim();

    // Status 0 is this client's own record of "nothing came back" — the fetch threw, so there is no status to map. It is
    // the one case where "nothing has changed" is certain, and it should read the same however the failure arrived.
    if (error.status === 0) {
      return {
        body: 'The app could not reach the hub. Check your connection — nothing has changed.',
        permanent: false,
      };
    }

    if (error.status === 401) {
      return { body: 'Your session has ended. Sign in again to carry on.', permanent: true };
    }
    // The platform's own words come first: they are written for a person and they know more than the status does
    // ("only an admin can see these rules" beats "you do not have access"). The status copy below is the fallback for
    // the client's own placeholder, which is what a message looks like when the server said nothing useful.
    if (said && !GENERIC.test(said)) {
      return { body: said, permanent: error.permanent };
    }
    if (error.status === 403) {
      return {
        body: 'You do not have access to this any more. Ask an admin of this home.',
        permanent: true,
      };
    }
    if (error.status === 404) {
      return { body: 'It may have been removed. The rest of your home is still there.', permanent: true };
    }
    return {
      body: 'The hub could not answer. Nothing has changed — try again in a moment.',
      permanent: false,
    };
  }

  // No status at all: the request never arrived, or the answer never came back.
  return {
    body: 'The app could not reach the hub. Check your connection — nothing has changed.',
    permanent: false,
  };
}
