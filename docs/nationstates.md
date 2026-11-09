# NationStates integration and security notes

Drumfire’s NationStates integration is deliberately small: **one public-shard
read, on demand, with no credentials.** This document records the policy and the
threat model so future features do not drift into unsafe territory.

Primary sources:

- [NationStates API documentation](https://www.nationstates.net/pages/api.html)
- [One Stop Rules Shop / Script Rules](https://forum.nationstates.net/viewtopic.php?f=16&t=260044)
- [Terms & Conditions](https://www.nationstates.net/pages/legal.html)
- [FAQ](https://www.nationstates.net/page=faq)

---

## 1. There is no general API key

NationStates does not issue an API key for public data. The only thing called an
“API key” is the **Telegrams API Client Key**, which is region-scoped and only
for telegram delivery.

| Capability | Authentication |
|---|---|
| Public nation/region/world/WA shards | None |
| Private shards (issues, dossier, notices, etc.) | `X-Pin`, `X-Password`, or `X-Autologin` over HTTPS |
| Private commands (issue, RMB, dispatch, cards, etc.) | Same, plus prepare/execute token |
| Telegrams | Region Client Key + telegram `TGID` + secret key |
| Third-party nation ownership check | Verification API (`a=verify` + checksum + optional site token) |

Drumfire only uses the first row. It does not collect, store, or transmit any
NationStates credential. The desktop UI says this explicitly.

**If authenticated features are ever added:** do not accept a password on a
server, do not store one in a config file, and do not use a hosted proxy for
write actions. Prefer the Verification API for identity and the OS keychain for
any local secret. That is a future design decision, not part of v1.

---

## 2. The request Drumfire makes

```text
GET https://www.nationstates.net/cgi-bin/api.cgi
    ?nation=<url-encoded-name>
    &q=name+fullname+region+population+category+motto+currency+flag
    &v=13
```

- One HTTP request, multiple shards.
- API version pinned to `13`. NationStates supports the two most recent
  versions; bump this deliberately after reading the changelog.
- The nation name is trimmed, length-checked, stripped of control characters,
  and URL-encoded. Spaces become underscores.
- The response is parsed as XML with `quick-xml`; text and entity references are
  resolved, and the fields are extracted into a typed `NationSummary`.
- A missing required field (`NAME`, `POPULATION`) is an error, not a silent zero.

The request is made on a background thread in the GUI, so the UI never blocks on
HTTP. Errors are surfaced as text next to the import controls.

---

## 3. User-Agent and contact

Every request sends:

```text
Drumfire/<version> (NationStates roleplay tool; contact: <contact>)
```

Set `DRUMFIRE_CONTACT` to a URL or email address that NationStates staff can use
to reach you. The fallback is the upstream project URL; for a public release,
set a real contact before distributing binaries.

A missing or misleading User-Agent is a violation of the API terms. The client
does not spoof a browser, and it does not change the User-Agent per request.

---

## 4. Rate limiting

The documented limit is **50 requests per 30 seconds per IP**, enforced with a
flush bucket. The API returns:

- `RateLimit-Policy: 50;w=30`
- `RateLimit-Limit`
- `RateLimit-Remaining`
- `RateLimit-Reset`
- `Retry-After` on `429`

Drumfire’s `RateGate`:

- starts with a conservative 50 / 30 s window;
- parses `RateLimit-Policy` and the other headers after every response;
- refuses a request with `NsError::RateLimited { retry_after }` when the local
  budget is exhausted, rather than sleeping the UI;
- surfaces a server `429` with its `Retry-After` value;
- is process-local and shared by all requests in that process.

Because the limit is per IP, a hosted multi-user service would need one global
queue and would still be constrained to ~1.67 requests/second. Drumfire avoids
that problem by being a local desktop tool: a user clicks “Fetch” for one nation
at a time. If a hosted mode is ever added, it must not simply fan out per-user
requests from one IP.

Drumfire does **not**:

- split requests across IPs or proxies;
- use puppet nations to obtain extra Telegram keys;
- retry a `429` in a tight loop;
- scrape HTML pages;
- download dumps more than once a day (no dump feature yet);
- enumerate a region’s nations with per-nation API calls.

Bulk region data, if added, must come from the [daily data dumps](https://www.nationstates.net/pages/api.html#dumps),
with `If-Modified-Since` and a 24-hour refresh floor.

---

## 5. What the import actually does

The GUI shows the nation’s short name, full name, region, population, category,
motto, currency, and flag URL. It then suggests:

```text
forces = clamp(population / 1000, 1, 10_000_000)
```

This is a **roleplay aid**, not a NationStates military statistic. NationStates
is a country simulator, not a military simulator; population is not troop count.
The user can edit the force count and every rating after import. The UI labels
the suggestion as such.

No imported value is sent back to NationStates. The import only reads.

---

## 6. Privacy and data handling

- No account login, password, PIN, autologin token, or API key is collected.
- No telemetry, analytics, crash reporting, or “phone home.”
- The fetched nation data lives in memory for the session and in the battle log
  only if the user runs and saves/exports a battle.
- The GUI does not write a cache to disk.
- The CLI prints to stdout; it does not log headers or credentials.
- The `User-Agent` contains only the app version and the contact you configure.

Battle logs contain only battle inputs, the seed, rolls, casualties, and
results. They contain no credentials and no network data.

---

## 7. Site rules that still apply

Even a read-only tool must respect the site’s rules:

- **No commercial use or monetization** on NationStates. Drumfire is free and
  open source.
- **No API-limit evasion**, IP splitting, or misleading User-Agent.
- **No nation hijacking or credential phishing.** Drumfire never asks for a
  password.
- **One WA nation per player.** Drumfire does not interact with the WA.
- **No impersonation** of NationStates, its staff, or its branding. The README
  and UI carry a non-affiliation notice.
- **No HTML automation.** The Script Rules treat API-only tools as compliant;
  Drumfire stays on the API.
- **Copyright.** Site content belongs to Max Barry; user content belongs to its
  authors. Drumfire does not republish either beyond the single nation lookup
  the user requests.

If a future feature would send content to NationStates (dispatch, RMB,
telegram), that content must also comply with the PG-13 content rules. That is a
reason to keep v1 read-only.

---

## 8. Verification checklist

Before shipping a build that touches the API:

- [ ] `User-Agent` includes the app name, version, and a real contact.
- [ ] `DRUMFIRE_CONTACT` documented and set for release builds.
- [ ] No credential field exists in the UI or config.
- [ ] No request logs headers or bodies containing secrets.
- [ ] Rate gate reads headers; `429`/`Retry-After` handled.
- [ ] API version pinned and changelog watched.
- [ ] No HTML scraping, IP rotation, or bulk per-nation loops.
- [ ] Non-affiliation notice present.
- [ ] `cargo test` and `cargo clippy -- -D warnings` pass.

---

## 9. Verification performed

- Unit tests cover XML parsing, entity unescaping, name normalization, force
  suggestion bounds, rate-gate exhaustion/reset, and header observation.
- A live read-only request for `testlandia` returned the expected nation fields
  with the project User-Agent.
- No live write or authenticated request has been made, by design.
