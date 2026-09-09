# ndn-dashboard redesign — plan

**Status:** Draft, authored 2026-08-26. Companion to `dashboard-next-salvage.md`
(what to keep from the archived rewrite) and `ARCHIVE.md` (what was demolished).
Where the salvage doc is an *inventory*, this is the *plan*: the target
architecture, the information architecture, and a gated phasing that does not
stall on the Anchor-concept for the work that doesn't need it.

---

## 1. Why redesign

The current `ndn-dashboard` (Dioxus, git-tag deps) predates most of the NDN-rust
stack it is supposed to operate, and its architecture shows it:

- **It never actually ran on the web target.** The first browser run this week
  panicked in wasm at `CertInfo::days_to_expiry()` → `std::time::SystemTime::now()`
  ("time not implemented on this platform"). A whole class of `std::time` /
  `std::fs` / `std::env` calls sit un-gated on the web render path (`logs.rs`
  save-to-file, `tool_runner::chrono_now`, core `types.rs`). The web build
  "compiled" and was declared working, but compilation never exercised the panic.
  Lesson: **wasm-cleanliness must be enforced, not assumed.**
- **A 35-view flat list with a monolithic shell.** `views/` holds 35 modules and
  the desktop shell `app.rs` is a single large file (the archived rewrite's
  equivalent was 5,842 LOC / 61 `rsx!` blocks — the salvage doc calls it "the
  strongest evidence against itself"). There is no small, NDN-conceptual
  information architecture; there is a pile of panels.
- **It is organized around forwarder plumbing, not NDN concepts.** Faces, routes,
  strategy, CS are present, but there is no first-class notion of *presence /
  trust*, *distributed traces*, *self-actuating cognition observed rather than
  configured*, or *services* — the things the stack now does.
- **The capabilities it predates** (all shipped or specced since):
  - **ControlSurface / `ext`** — generic subsystem introspection served at
    `/localhost/nfd/ext/list` (this session wired radio-cognition telemetry
    through it; the mechanism is general).
  - **NDN-native observability** — OTLP `Span` in Data + the per-hop
    **TraceContext LP TLV** (`0x520`, `trace-context-lp-tlv.md`). Distributed
    tracing with no out-of-band path.
  - **Persistent Interests** — the **SubscriptionRequest sub-TLV** (`0x230`,
    `subscription-request-sub-tlv.md`): one PIT entry satisfied by many Data.
    This is the right substrate for *live* panels (traces, traffic, cognition)
    instead of poll loops.
  - **Service layer** — NDNSF-/NAC-ABE-compatible service discovery, four-phase
    exchange, SVS v3 sync (`service-layer.md`, gated by `ndnsf-invariants.md`).
  - **did:ndn** — certificates resolved over NDN as W3C DIDs (`did-ndn-spec.md`);
    the trust model has a spec now.
  - **named-time** — trusted, uncertainty-bounded time (`ndn-time` /
    `ndn-timekeeper`), a cognition control plane like named-radio.
  - **Anchor / Ripple** — presence + nearby-peers over NAN + BLE, on
    `ndn-mobile` / `ndn-boltffi`.

The standing ruling (salvage doc §intro): *"all existing UI/UX in this workspace
is weak and the crate is a parts bin."* This plan treats presentation as
replaceable and logic as the asset.

## 2. Gate and non-negotiables

- **The rebuilt *product* is gated on the Anchor-concept** (`STATE.md:39-43`):
  the dashboard's model of *which nodes exist, how they are discovered, paired,
  and trusted* must come from the Anchor foundation (the discovery dataset,
  pairing, did:ndn) — not an ad-hoc capability/posture sketch. Until that model
  is concrete, **the presence/trust spine (Phase 2) waits.** Everything upstream
  of it does not — see the phasing.
- **Observe cognition; do not reconfigure it by hand.** The named-radio (and
  named-time) control planes *actuate themselves*. The dashboard shows the
  *decided* channel/rate/power/FEC/objective and *why*; it does not offer manual
  radio knobs. (User ruling, this project.)
- **NDN concepts, not IP.** Names, faces, prefixes, trust anchors, services,
  presence — not interfaces, subnets, DHCP.
- **Reads are public/unsigned; mutations are signed** with an operator identity.
- **wasm-first, wasm-clean.** The router deployment is a browser SPA; desktop is
  a superset, not the primary. No `std::time`/`std::fs`/`std::env` on the shared
  or web path — enforced by a guard test (see §6).

## 3. Target architecture — three crates, clean seams

```
            ┌─────────────────────────────────────────────┐
            │  ndn-dashboard (shell)  — thin Dioxus         │
            │  NDN-native IA, component lib, zero logic      │
            │  wasm-first; desktop = superset               │
            └───────────────┬───────────────────────────────┘
                            │ (models + commands only)
            ┌───────────────▼───────────────────────────────┐
            │  ndn-dashboard-core (headless operator logic)  │
            │  engine poll · mgmt (WS + IPC) · operator       │
            │  keyring/custodian · trust/audit chains ·       │
            │  ext/ControlSurface ingest · service client ·   │
            │  presence/discovery ingest (Phase 2)            │
            └───────────────┬───────────────────────────────┘
                            │ (wire ingest + viz models)
            ┌───────────────▼───────────────────────────────┐
            │  ndn-viz (NEW, shared with ndn-sim)            │
            │  OTLP decoder · observability fetch contract ·  │
            │  trace/span view-model algebra · dataset        │
            │  freshness model · (future) TraceContext ingest │
            │  UI-agnostic, wasm-clean, no_std-friendly       │
            └─────────────────────────────────────────────────┘
```

### 3.1 `ndn-viz` — the shared data-visualization layer (NEW)

The salvage doc points every reusable observability piece at a *"shared
data-visualization layer (ndn-sim + dashboard)."* This crate is that layer. It is
UI-agnostic (zero Dioxus imports), wasm-clean, and consumed by **both** the
dashboard and `ndn-sim` (which already produces spans these functions can shape).

**Home (decided):** a shared **utility crate in `ndn-ext`**, alongside
`ndn-dashboard-core` (e.g. `ndn-ext/crates/dashboard/ndn-viz`) — not its own
repo. `ndn-sim` (a separate workspace) consumes it by git tag, the same way the
dashboard consumes `ndn-dashboard-core` today. One place to cut tags, no new repo
to track.

Contents, lifted from the archived `ndn-dashboard-next` (recover with
`git checkout a81e1c8 -- crates/ndn-dashboard-next`) **and** from what this
session already built in the app crate (fold it down here, don't leave it in the
shell):

| Module | Source | What |
|---|---|---|
| `otlp` | salvage `observe/mod.rs:761-966` + this session's `views/trace_otlp.rs` | dependency-free OTLP `Span` protobuf decoder → flat `SpanView`, pulling `ndn.target` / `interest.name` / `face.id` / `strategy.name` |
| `observe` (source) | salvage `observe/mod.rs:346-435,723-725` | the `/recent` listing parser + span-name builder — the consumer half of the `/localhost/nfd/observability` naming contract |
| `trace_model` | salvage `observe/mod.rs:470-759` (13 tests) | pure `group_spans` / `span_tree_rows` / `pit_fanout_rows` / `filter_traces` / log-correlation — the model tier, cycle-safe and deterministic |
| `source` (freshness) | salvage `engine.rs:260-473` | `DatasetSource{Fresh\|Stale\|Disconnected\|Unsupported}` per-panel provenance + RIB-preferred/FIB-fallback — as a **trait** every data source implements |
| `tracecontext` (future) | `trace-context-lp-tlv.md` (0x520) | decode per-hop LP trace headers so the timeline can show hop spans without the OTLP round-trip |

**Value of doing this first:** it is useful *regardless of the Anchor gate* and it
unblocks `ndn-sim` too. It also validates the shell/core/viz seam before we invest
in a new shell.

### 3.2 `ndn-dashboard-core` — keep and extend

The existing headless core (`ndn-ext/crates/dashboard/ndn-dashboard-core`) is the
asset to build on — it already has `engine`, `mgmt`, `identity_axis`, `keyguard`,
`operator_keyring`(+ store), `security_chains`, `signed_data_chain`,
`forwarder_profile`, `native_join`, `preprovision`, `types`. Changes:

- **Fix wasm-cleanliness** (started this session: `types.rs::days_to_expiry` now
  routes through a cfg-split `now_unix_s()` using `js_sys::Date::now()` on wasm).
  Sweep the rest; add the guard test.
- **Depend on `ndn-viz`** for observability models instead of carrying its own.
- **Generalize `ext`/ControlSurface ingest** — this session added a radio-only
  `CognitionPanel` fetch in the app crate; move it to core as a generic
  `poll_ext()` that parses every `[subsystem]` section (radio today, time/coding
  tomorrow) into a typed `ControlSurfaces` model. (The `ext/list` public-read
  auth change already landed in `ndn-mgmt`.)
- **Add a service-layer read client** (`service-layer.md`): service discovery +
  four-phase exchange status, read-only.
- **Hardware & health ingest** — extend the `ext`/ControlSurface reporting beyond
  *decided plans* to the *substrate*: per-node system health; per-radio device
  identity + addressing (`bus:port`, driver, chip); capability (bands/channels,
  MCS/NSS/BW/HE, RX chains, dBm power path, timestamp/TSF); and live PHY health
  (link up/down via the sysfs guard, USB-bus health, power/brownout, per-neighbor
  RSSI, channel/spectrum occupancy from frame-free sensing, TX/RX counters, and
  SDR if attached). This is a distinct `HardwareInventory` model from the
  cognition *decisions* — the dashboard shows the substrate **and** what cognition
  decided on it.
- **Monitors model** — a `Monitors` model over long-running watchers (soak runs,
  uptime/health watchdogs, link-quality-over-time, threshold alerts): each
  watcher's status, duration, pass/fail, and wedge/recovery history. It is
  cross-cutting — aggregating Observability (span history), Hardware (health),
  Cognition (objective/suppression), and Forwarding (counters). Source: the
  soak/health datasets + the observability `/recent` history.
- **Live datasets:** poll + per-dataset freshness for Phase 0/1; **persistent
  Interests (`0x230`) adopted in Phase 3** (owner decision) so panels stream
  instead of poll — behind the existing `mgmt` seam.
- **Presence/discovery ingest (Phase 2, gated):** the discovery dataset + pairing
  + did:ndn identity model, sourced from the Anchor foundation, not invented here.

### 3.3 `ndn-dashboard` — rebuild the shell

A thin Dioxus shell: a small component library + the new IA (§4), **no monolithic
`app.rs`, no business logic, no mock clients** (the three things the salvage doc
convicts). wasm-first so it serves from the router; desktop is the same shell with
extra capabilities (OS keychain, file export, multi-window) gated behind
`#[cfg(not(target_arch = "wasm32"))]` at the *platform-service* boundary, never in
render code.

Transport lessons from this session, baked in:
- **localhost scope (decided — the foolproof path):** `/localhost/nfd` is dropped
  from a non-loopback WS face. Rather than depend on `/localhop` semantics
  (forwarder-version-dependent, unproven here), standardize on what is proven:
  **bind the WS face to the router's loopback and reach it through a single
  loopback-terminating SSH tunnel** so the connection originates from the router's
  `127.0.0.1`. Ship one connection field defaulting to `ws://localhost:9696` plus
  a scripted tunnel. The zero-tunnel end state is **on-router same-origin hosting
  (Phase 4)** — serve the SPA from the router so the browser and the WS face share
  an origin and no tunnel exists at all.
- **Dioxus/dx**: pin `dioxus = 0.7.x`; `dx` **must run from the workspace root**
  (`dx serve --platform web -p ndn-dashboard --no-default-features --features web`)
  — from the crate subdir it panics `workspace.rs:325`.

## 4. Information architecture — NDN concepts, not a panel pile

Collapse the 35-view list into six conceptual sections. Each maps existing views
to a home and names the new surfaces.

1. **Presence & Trust** *(Phase 2, Anchor-gated)* — nearby peers from the
   discovery dataset, pairing, did:ndn identities, trust anchors, enrollment,
   CA approvals. *(maps: `identity_axis`, `keyguard`, `trust_context`,
   `security_did`(+`_ext`), `pairing`, `onboarding`, `ca_approvals`,
   `enrollment_wizard`, `safebag_import`, `key_rotation`)*
2. **Names & Forwarding** — faces, RIB/FIB routes as *prefixes*, strategy per
   prefix, content store. The forwarding table shown as names, with per-dataset
   freshness. *(maps: `faces`, `routes`, `routing`, `strategy`, `cs`)*
3. **Observability** — distributed traces (OTLP + TraceContext timeline), span
   trees with PIT-fanout, live traffic, correlated logs. Powered by `ndn-viz`.
   *(maps: `trace`(+`_otlp`), `traffic`, `logs`, `inspector`)*
4. **Cognition** — the `ext`/ControlSurface observatory: named-radio (decided
   channel/rate/power/FEC/objective, read-only), named-time, coding, rate-limit.
   Observe self-actuating control planes; no manual radio config.
   *(maps: `radio`, `coding`, `rate_limit`)*
5. **Hardware** *(new)* — the physical substrate: node inventory (which SBCs are
   up, role, uptime, resource use) and, per node, the attached-radio inventory
   (chip/driver, stable `bus:port` address), each radio's capability
   (bands/channels, MCS/NSS/BW/HE, RX chains, dBm power path, TSF/timestamp) and
   live PHY health (link up/down, USB-bus health, power/brownout, per-neighbor
   RSSI, channel/spectrum occupancy, TX/RX counters, SDR if present). Shows the
   ground truth the cognition acts on — distinct from §4.4's *decisions*.
   *(fed by the Hardware & health ingest in §3.2; the raw-substrate side of
   `radio` + fleet device data)*
6. **Monitors** *(new)* — long-running watchers and their history: soak runs
   (status, duration, pass/fail, wedge/recovery), uptime/health watchdogs,
   link-quality-over-time, and threshold alerts. Cross-cutting — reads the
   Monitors model (§3.2), which aggregates Observability, Hardware, Cognition,
   and Forwarding. This is where the overnight-soak surface and alerting live.
7. **Services** *(new)* — NDNSF service discovery, four-phase exchange status,
   NAC-ABE access grants, subscriptions. From the service-layer client.
8. **Node** — status/general, config, session, tools, compose, fleet, settings.
   *(maps: `overview`, `config`, `dashboard_config`, `session`, `tools`,
   `compose`, `fleet`, `settings`)*

## 5. Phasing — decoupled from the Anchor gate where possible

- **Phase 0 — Extract `ndn-viz` (ungated, do now).** Create the shared utility
  crate (in ndn-ext) from salvage §1.1-1.4 + this session's
  `trace_otlp`/`trace`/cognition work; point `ndn-sim` at it too. Land the
  wasm-clean guard test. The **current shell keeps running** on it — proving the
  seam with no rebuild and no dependence on the Anchor model.
- **Phase 1 — Core capability expansion (ungated).** In `ndn-dashboard-core`:
  generic `ext` ingest, **Hardware & health ingest + Monitors model**,
  service-layer read client, TraceContext ingest. Live data stays **poll +
  per-dataset freshness** (no subscriptions yet). All read-only; validated against
  a live `ndn-fwd` over the WS loopback face (this session's harness). The current
  shell consumes the new models directly, so the new Hardware/Monitors data is
  usable before any shell rebuild.
- **Phase 2a — Anchor readiness reassessment (GATE, do before 2b).** Revisit the
  Anchor concept **from the ground up** — `ndn-anchor`, `ndn-ripple`,
  `ndn-mobile`/`ndn-boltffi`, the discovery dataset, pairing, and the did:ndn
  attach/capability model — to determine its actual state and readiness. Output: a
  concrete attach/capability contract the dashboard can bind to, or a verdict that
  it is not ready (in which case 2b/3 stay parked). **Nothing downstream starts
  until this lands.**
- **Phase 2b — Presence & Trust spine (GATED, needs 2a).** Discovery dataset +
  pairing + did:ndn identity model in core, bound to the 2a contract. Do not
  hard-couple to a provisional model.
- **Phase 3 — New shell (HELD until Phase 2 is unblocked — owner decision).** Thin
  Dioxus components over core/viz, the full §4 IA shipped **whole** (Presence &
  Trust arrives with the rest, not bolted on later), wasm-first, router-hostable.
  Adopt **persistent Interests (`0x230`)** here for live panels. Mutations with
  operator signing.
- **Phase 4 — On-router hosting.** uhttpd + precompressed WASM (the zero-tunnel
  same-origin end state); begin LuCI disposition beyond the NDN-primary hybrid.

## 6. Cross-cutting: enforce wasm-cleanliness (the crash lesson)

- One platform-services module with cfg-split clock/fs/export helpers
  (`now_unix_s`, downloads via a browser blob on wasm, OS file on desktop). No
  `std::time`/`std::fs`/`std::env` anywhere else on the shared/web path.
- **Guard test** (source-scanning, cheap — the technique the archived crate's
  `architecture_guards.rs` used): ban those tokens outside the platform module in
  `ndn-viz`, `ndn-dashboard-core`, and the shell's non-`cfg(desktop)` code.
- CI builds **and smoke-loads** the wasm (headless), not just `cargo check`, so a
  render-path panic fails the build.

## 7. Dev workflow (so this is buildable without pain)

The dashboard consumes `ndn-dashboard-core` by **git tag** (`v0.1.0-alpha.3`).
This session hit two walls: (a) a `[patch]` to the local core dragged in a second
`ndn-packet` (ndn-ext workspace vs the tag's resolved graph) → type-identity
errors; (b) editing the cargo git checkout works but is not reproducible. Decide
one of:
- a **dev `[patch]` set** that repoints core *and its ndn-rs/ndn-ext deps*
  consistently, or
- move the dashboard onto the workspace **path-dep** graph for development and cut
  tags for release.

Pick this before Phase 0 lands, or every core change is a checkout hack.

## 8. What we explicitly do NOT carry forward

Per the salvage doc §2: the entire Dioxus shell, the mock "typed clients"
(`client/mod.rs`), the capability/posture state model (`core/mod.rs` — the
Anchor-concept supersedes it), mutation preflight as code (the UX idea survives,
the code doesn't), the tools/network/audit/extensions view-model sketches, and the
PWA plumbing. Trust ceremonies (`identity/mod.rs`) are kept as *reference for the
call sequence* only.

## 9. Decisions (resolved 2026-08-26 by the owner)

1. **`ndn-viz` home** — a shared **utility crate in `ndn-ext`** (alongside
   `ndn-dashboard-core`), not its own repo; `ndn-sim` consumes it by tag. (§3.1)
2. **Anchor-concept readiness** — **revisit Anchor from the ground up** to
   determine its state/readiness before the presence/trust spine. This is now
   **Phase 2a**, the gate for everything downstream. (§5)
3. **Live transport** — **poll + freshness for Phase 0/1; adopt persistent
   Interests (`0x230`) in Phase 3.** (§3.2, §5)
4. **Connectivity** — **simple, proven, robust: the WS face on the router's
   loopback + a single loopback-terminating SSH tunnel**; no `/localhop`
   dependence. On-router same-origin hosting (Phase 4) is the zero-tunnel end
   state. (§3.3)
5. **First shippable** — **hold the shell rebuild until Phase 2 is unblocked** and
   ship the §4 IA **whole**. Phases 0/1 land `ndn-viz` + the expanded core (incl.
   Hardware & Monitors) under the *current* shell in the meantime. (§5)
