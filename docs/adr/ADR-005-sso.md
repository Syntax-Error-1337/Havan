# ADR-005 — SSO: configuration now, enforcement with the provider

**Status:** accepted, partially implemented

## The problem

A vendor replacing their own cloud asks for SSO in the first meeting. What they need from us is
small: a place to record "we require SSO", a place to record which identity-provider subject belongs
to which account, and a refusal to bind one identity to two people.

What they need from us that we cannot fake is the assertion: validating a SAML or OIDC response
against their provider, with the provider's key, clock and audience rules.

## Decision

**Store and expose the configuration; do not enforce it yet.**

`vendors.sso_required`, `vendors.sso_domain` and `vendor_users.sso_subject` exist, with a unique
index so one identity cannot be bound to two accounts. `GET /v1/vendor/sso` reports the state, and
includes `enforced: false` — the response says out loud that requiring SSO does not yet change how
anyone signs in.

Enforcement is deliberately not wired to the login path. The moment `sso_required` disables password
login, every member without a bound subject is locked out of the portal — and with no assertion to
present, being locked out is permanent. Shipping that would be a worse failure than the gap it
closes, and it would happen to the first vendor who took the feature seriously.

## What enforcement needs, when it lands

1. An OIDC or SAML callback that verifies the assertion against the vendor's configured issuer, its
   signing key and its audience, with a replay window.
2. Login consults `sso_required`: password refused for a user with a bound subject; a member with no
   subject is refused with "ask your owner to bind your identity" rather than a password prompt.
3. Enabling `sso_required` should refuse while any member is unbound — the check that prevents an
   owner from locking a colleague out with a toggle. The data for that check is already reported by
   `GET /v1/vendor/sso` (`bound_members` vs `total_members`).
4. The provider secrets live per vendor, encrypted, with the same discipline as the push credentials.

## Consequences

- A vendor can be shown, honestly, what is ready and what is not: the flag, the binding, and the
  count of who is not covered.
- No lockout risk, and no false claim of protection.
- The remaining work is one integration, not a redesign: the tables, the endpoints and the policy
  checks are in place and tested.
