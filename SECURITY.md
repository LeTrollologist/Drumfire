# Security policy

## Supported versions

Drumfire is pre-1.0. Security fixes land on the latest `0.4.x` release and on
`main`.

## Reporting a vulnerability

Please do **not** open a public issue for a security problem. Use the
repository’s private security advisory feature, or contact the maintainer
through the contact listed in the repository profile.

Include:

- the affected version or commit;
- a minimal reproduction;
- the impact you believe it has;
- whether any NationStates credential or API limit is involved.

You should receive an acknowledgement within a few days. Please give a
reasonable window to fix the issue before disclosing it publicly.

## Scope

In scope:

- credential handling (the project currently collects none);
- NationStates API compliance, including rate-limit bypass and User-Agent
  spoofing;
- parsing of untrusted battle logs or API responses;
- denial of service through malformed input;
- dependency and supply-chain issues;
- unsafe file handling in the GUI or CLI.

Out of scope:

- NationStates account recovery, moderation, or in-game disputes;
- issues in NationStates itself (report those to the site);
- social engineering that requires the user to hand over a password;
- vulnerabilities in third-party crates that are already publicly known and
  have an upstream fix (report them upstream, and open a dependency update here).

## Design commitments

- No NationStates password, PIN, autologin token, or Telegram key is collected.
- The only network request is a read-only public-shard lookup.
- The API `User-Agent` identifies the tool and a configurable contact
  (`DRUMFIRE_CONTACT`).
- The rate gate is intentionally conservative and header-aware; it must not be
  changed to evade the per-IP limit.
- Battle logs are untrusted input: validate schema and size before replay.
- Do not add telemetry, analytics, or crash reporting that includes battle or
  nation data without a documented opt-in.

## API-key policy

There is no general NationStates API key. If a future feature needs
authentication, it must use the NationStates Verification API or the operating
system keychain, never a server-side password store. Any such change requires a
new threat-model review and an update to this document.
