# ndn-dashboard-next — salvage inventory

`crates/ndn-dashboard-next` (15,044 LOC) was the mock-backed milestone-one
dashboard rewrite. Last touched 2026-06-18 (`bd2859f`); archived 2026-08-21
under the P2 demolition plan. This is a skeptical inventory: the owner's
standing ruling is that all existing UI/UX in this workspace is weak and the
crate is a parts bin — salvage below had to be earned with concrete evidence,
and most of the crate does not earn it.

Headline: roughly 1,200 of 15,044 LOC earn salvage, almost all of it in
`src/observe/mod.rs`. The rest is a mock-backed presentation and state-model
sketch that the Anchor-concept gate will obsolete anyway.

All line numbers refer to the tree as of the pinned SHA in section 3.

## 1. EARNS SALVAGE

### 1.1 OTLP span decoder — `src/observe/mod.rs:761-966`

`decode_otlp_span` + `ProtoReader` + `decode_attr` / `decode_any_value` /
`decode_status`. A dependency-free, hand-rolled protobuf reader that decodes
the OTLP `Span` message (trace/span/parent ids, name, fixed64 start/end,
attributes, status) into a flat `SpanView`, pulling out the NDN-specific
attributes `ndn.target`, `interest.name`, `face.id`, `strategy.name`.

Evidence of value:
- It decodes exactly what `ndn-rs/crates/platform/ndn-observability/src/publisher.rs`
  publishes (`Span::encode()` OTLP protobuf inside unsigned Data under
  `<prefix>/traces/<trace-id-hex>/spans/<span-id-hex>`). It is the **only
  consumer-side decoder of that wire in the entire workspace** — grep for
  `decode_otlp` / OTLP protobuf parsing finds the publisher (encoder), the
  ndn-sim OTLP/JSON exporter, and this file. Nothing else can read the spans
  ndn-fwd publishes.
- Unit-tested with hand-encoded protobuf round-trips
  (`src/observe/mod.rs:1023-1045`), including bounds-checked varint/len/fixed64
  reads that return `DecodeError::Malformed` instead of panicking.

Destination: **shared data-visualization layer** (ndn-sim + dashboard).
`ndn-dashboard-core` in ndn-ext is the fallback home if the viz layer grows
its own wire-ingest module elsewhere.

### 1.2 Observability fetch contract (consumer side) — `src/observe/mod.rs:346-435, 723-725`

`parse_recent_listing` (413-426), `span_data_name` (428-435),
`is_hex_of_len` (723-725), plus the live fetch loop `fetch_desktop_spans`
(361-397: `ndn_app::Consumer` fetch of `<prefix>/recent`, then per-span
fetch + decode, capped at `MAX_RECENT_SPANS`) and `fetch_desktop_recent_logs`
(346-359: `ndn_ipc::MgmtClient::log_get_recent`).

Evidence of value:
- Encodes the consumer half of the `/localhost/nfd/observability` naming
  contract whose producer half lives in ndn-rs (`encode_recent` emits the same
  newline-separated `trace-hex32/span-hex16` listing this parses; the name
  shape matches the publisher's doc comment verbatim).
- Malformed-input rejection is tested (1048-1068): non-hex and wrong-length
  ids fail with `MalformedRecent` rather than producing garbage names.
- This is real I/O, not mock: the `tests/observe_witnesses.rs` witnesses run
  it against a live ndn-fwd socket.

Destination: **shared viz layer's data source** (or the legacy dashboard's
data layer if it grows a spans view first).

### 1.3 Trace/span view-model algebra — `src/observe/mod.rs:470-759`

Pure, presentation-free functions over `SpanView`/`TraceView`:
- `group_spans` (727-759) — flat span list → per-trace groups with duration
  accumulation and PIT-fanout flagging.
- `span_tree_rows` + `push_span_tree_row` (548-599, 641-672) — parent/child
  tree flattening with deterministic ordering, orphaned-parent detection, and
  a visited-set so cycles/duplicates cannot loop.
- `pit_fanout_rows` (674-687), `filter_traces` (470-484: multi-term AND
  filter across every operator-facing field), `correlated_logs_for_trace` +
  `trace_log_needles` (486-532: log-line-to-trace correlation with
  `matched_by` provenance labels), `bridge_status_from_logs` (601-639).

Evidence of value: 13 unit tests (998-1196) cover grouping, tree building,
orphan handling, filtering, log correlation, and bridge-status derivation.
Zero UI imports — this is exactly the model tier the shared viz layer needs,
and ndn-sim already produces spans these functions could shape.

Destination: **shared data-visualization layer**.

### 1.4 Live engine dataset poll with per-dataset freshness — `src/engine.rs:260-473`

`poll_desktop_engine_summary`: one `ndn_ipc::MgmtClient` session issuing
`status` / `face_list` / `rib_list` / `route_list` / `strategy_list`, mapping
into typed rows, with two ideas the legacy dashboard does not have:
- per-dataset `DatasetSource { state: Fresh | Stale | Disconnected | Unsupported }`
  provenance for every panel (371-409), instead of one global connected flag;
- RIB-preferred-with-FIB-fallback route listing (315-340), so read-only
  targets still show routes.

Evidence of value: real I/O exercised by the live witness in
`tests/attach_witnesses.rs`; the mapping helpers (`face_scope_label`,
`face_persistency_label`, `satisfaction_pct` with saturating math, 447-473)
are small but correct. Caveat, honestly stated: legacy already polls the same
verbs through its `ManagementClient` seam, so what earns salvage here is the
**dataset-freshness model and the fallback logic**, not the polling itself.

Destination: **legacy dashboard data layer** (and the freshness model as a
pattern for the shared viz layer's data sources).

### 1.5 In-process trust ceremonies — `src/identity/mod.rs` (weak pass, reference value)

- `execute_tofu_adoption` (764-790): `ndn_cert::BootstrapTicket::from_fragment`
  → mandatory out-of-band fingerprint confirmation → `ndn_cert::adopt_with_tofu`,
  with distinct error messages for missing OOB confirmation vs anti-rollback
  rejection.
- `execute_ndncert_enrollment` (821-~890): drives
  `ndn_identity::enroll::NdncertClient` end-to-end, including the
  token/possession/custom challenge-input mapping.
- Thin real adapters: `TrustContextSummary::from_security_keyring` (102-~190),
  `ValidationFrame::from_chain_trace` (467-496),
  `DidFrame::from_certificate` (523-533).

Evidence — and the caveat that makes this a weak pass: the legacy dashboard
performs TOFU/enrollment/DID through forwarder mgmt verbs, not in-process
library calls; grep finds **no other in-repo caller** of `adopt_with_tofu`,
`NdncertClient`, `validator::ChainTrace`, or `cert_to_did_document` in a
dashboard context. But each adapter is under ~60 LOC of glue over APIs that
live (tested) in ndn-rs. Salvage these as **reference for the call sequence**,
not as transplantable modules — recovering them via the SHA below when needed
is cheaper than carrying them.

Destination: reference for **ndn-dashboard-core** / whatever the
Anchor-gated dashboard becomes.

### 1.6 Patterns worth stealing (no code transplant)

- Live-forwarder witness tests (`tests/attach_witnesses.rs`,
  `tests/observe_witnesses.rs`): env-keyed socket
  (`NDN_DASHBOARD_NEXT_LIVE_NDN_FWD_SOCK`), `#[ignore]` with the setup script
  named in the ignore reason. Good discipline for any future dashboard crate.
- Source-scanning architecture guard (`tests/architecture_guards.rs`):
  cheap token-ban test enforcing a crate boundary. The specific tokens die
  with this crate; the technique is reusable.

## 2. DOES NOT EARN

Per the owner's ruling all presentation is presumed replaced; the burden of
proof was on each piece, and these fail it:

- `src/app.rs` (5,842 LOC, 61 `rsx!` blocks, inline CSS, one file) — the
  entire Dioxus shell. Presentation, monolithic, and the strongest evidence
  against itself: a "split-ready architecture" rewrite whose UI is one
  5.8k-line file.
- `src/client/mod.rs` (572 LOC) — the "typed clients" are **all mock**: every
  `DashboardClient` impl returns a hardcoded `ProbeTranscript`; no transport
  exists. The trait seam plus `normalize_probe`'s capability-degradation
  mapping were never validated against a real NFD/YaNFD and are re-derivable
  in an afternoon.
- `src/core/mod.rs` (678 LOC) — capability/posture state model
  (`CapabilitySet`, `FeatureState`, postures, fixtures). A design sketch
  tested only against its own fixtures; the Anchor-concept gate means the
  attach/capability model gets re-derived regardless.
- `src/mutation.rs` (1,726 LOC) — preflight gates + typed commands + execute
  wrappers. Execution is thin dispatch over `ndn_ipc::MgmtClient`'s already
  typed methods (`face_create_with_mtu`, …) — the real value lives in ndn-ipc,
  and legacy already issues the same verbs. The preflight-checklist UX is an
  idea, not scarce code.
- `src/tools/mod.rs` (804 LOC) — ping/iperf/peek/put workflows over
  `ndn_tools_core`, duplicating legacy's `tool_runner.rs` against the same
  APIs; plus `mock_runs` fixtures.
- `src/identity/mod.rs` minus §1.5 (~900 of 1,222 LOC) — fixture row
  generators and hardcoded postures.
- `src/operations.rs`, `src/config.rs`, `src/network.rs`, `src/audit.rs`,
  `src/extensions.rs` (1,619 LOC) — pure view-model/state sketches for the
  discarded shell; `extensions.rs` and `network.rs` are entirely static
  surface descriptions.
- `src/platform/mod.rs` (478 LOC) — trivial `PreferenceStore` impls plus a
  `Command::spawn` ndn-fwd process manager (find-binary-in-PATH, one global
  child). ndn-fwd's own tooling owns process lifecycle.
- `src/engine.rs:475-676` — `ndnrs_snapshot`/`compatible_snapshot` mock
  fixture data.
- `public/` (PWA manifest, `sw.js`, icon) and `src/main.rs` — shell plumbing
  for the discarded app.
- `tests/architecture_guards.rs` — guards a next/legacy boundary that no
  longer exists; the a11y-baseline half string-matches against the discarded
  `app.rs`.

## 3. HOW TO RECOVER

The archive commit's **parent** is the last commit containing the tree.
Pinned SHA known to contain the full, final tree (unchanged since `bd2859f`,
2026-06-18):

```
a81e1c8
```

Recover the whole crate:

```
git checkout a81e1c8 -- crates/ndn-dashboard-next
```

or read single files, e.g.:

```
git show a81e1c8:crates/ndn-dashboard-next/src/observe/mod.rs
```

Exact paths archived:

- `crates/ndn-dashboard-next/Cargo.toml`
- `crates/ndn-dashboard-next/src/lib.rs`, `src/main.rs`, `src/app.rs`,
  `src/audit.rs`, `src/config.rs`, `src/engine.rs`, `src/extensions.rs`,
  `src/mutation.rs`, `src/network.rs`, `src/operations.rs`
- `crates/ndn-dashboard-next/src/client/mod.rs`, `src/core/mod.rs`,
  `src/identity/mod.rs`, `src/observe/mod.rs`, `src/platform/mod.rs`,
  `src/tools/mod.rs`
- `crates/ndn-dashboard-next/tests/architecture_guards.rs`,
  `tests/attach_witnesses.rs`, `tests/observe_witnesses.rs`
- `crates/ndn-dashboard-next/public/manifest.webmanifest`, `public/sw.js`,
  `public/icons/ndn-dashboard-next.svg`
