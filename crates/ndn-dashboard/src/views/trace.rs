//! **Traces** — the observability view: fetch OTLP-in-Data spans from the
//! forwarder's `ndn-observability` publisher and render them as per-trace span
//! timelines. Read-only, unsigned: walks `<prefix>/recent` then per-span Data.
//!
//! This is the read side of `ndn-observability` — the forwarder publishes bare
//! OTLP `Span` protobufs under `/localhost/nfd/observability`; we fetch them and
//! decode via the shared, wasm-clean `ndn-viz` layer (the single consumer-side
//! OTLP decoder). Named-radio decide/actuate/apply spans ride the same path.

#[cfg(feature = "web")]
pub use web::Trace;

#[cfg(not(feature = "web"))]
#[dioxus::prelude::component]
pub fn Trace() -> dioxus::prelude::Element {
    use dioxus::prelude::*;
    rsx! {
        div { class: "placeholder", style: "padding:2rem;color:var(--text2);",
            "The trace/observability view is available in the web build."
        }
    }
}

#[cfg(feature = "web")]
mod web {
    use dioxus::prelude::*;

    use crate::app_web::default_ws_url;
    use crate::ws_mgmt::WsMgmtClient;
    use ndn_packet::Name;
    use ndn_viz::{SpanView, decode_otlp_span, parse_recent_listing};

    /// Observability dataset prefix on the forwarder (matches ndn-fwd default).
    const OBS_PREFIX: &[&[u8]] = &[b"localhost", b"nfd", b"observability"];
    /// Cap per refresh so the tunnel isn't flooded (newest-first from `/recent`).
    const MAX_SPANS: usize = 80;

    /// All spans sharing one trace-id, ready to render as a timeline.
    #[derive(Clone, PartialEq)]
    struct TraceGroup {
        trace_hex: String,
        spans: Vec<SpanView>,
        min_start: u64,
        max_end: u64,
    }

    fn obs_name(extra: &[&[u8]]) -> Name {
        let mut n = Name::root();
        for c in OBS_PREFIX {
            n = n.append(c);
        }
        for c in extra {
            n = n.append(c);
        }
        n
    }

    /// Fetch `/recent`, then each span, decode, and group by trace-id.
    async fn fetch_traces() -> Result<Vec<TraceGroup>, String> {
        let mut client = WsMgmtClient::new(&default_ws_url());
        client.connect().await.map_err(|e| e.to_string())?;

        let recent = client
            .fetch_data(obs_name(&[b"recent"]))
            .await
            .map_err(|e| format!("recent: {e}"))?;
        let refs = parse_recent_listing(&recent).unwrap_or_default();

        let mut rows: Vec<SpanView> = Vec::new();
        for r in refs.into_iter().take(MAX_SPANS) {
            // `/recent` yields hex ids directly; they are the Data name components.
            let name = obs_name(&[
                b"traces",
                r.trace_id.as_bytes(),
                b"spans",
                r.span_id.as_bytes(),
            ]);
            if let Ok(content) = client.fetch_data(name).await {
                if let Ok(row) = decode_otlp_span(&content) {
                    rows.push(row);
                }
            }
        }

        // Group by trace-id, order spans by start, compute timeline bounds.
        use std::collections::BTreeMap;
        let mut by_trace: BTreeMap<String, Vec<SpanView>> = BTreeMap::new();
        for r in rows {
            by_trace.entry(r.trace_id.clone()).or_default().push(r);
        }
        let mut groups: Vec<TraceGroup> = by_trace
            .into_iter()
            .map(|(trace_hex, mut spans)| {
                spans.sort_by_key(|s| s.start_unix_nano);
                let min_start = spans.iter().map(|s| s.start_unix_nano).min().unwrap_or(0);
                let max_end = spans
                    .iter()
                    .map(|s| s.end_unix_nano.max(s.start_unix_nano))
                    .max()
                    .unwrap_or(0);
                TraceGroup {
                    trace_hex,
                    spans,
                    min_start,
                    max_end,
                }
            })
            .collect();
        // Newest trace first.
        groups.sort_by(|a, b| b.max_end.cmp(&a.max_end));
        Ok(groups)
    }

    fn fmt_dur(nanos: u64) -> String {
        if nanos >= 1_000_000 {
            format!("{:.1} ms", nanos as f64 / 1e6)
        } else if nanos >= 1_000 {
            format!("{:.1} µs", nanos as f64 / 1e3)
        } else {
            format!("{nanos} ns")
        }
    }

    #[component]
    pub fn Trace() -> Element {
        // Bump to trigger a refetch; `use_resource` re-runs when it changes.
        let mut refresh = use_signal(|| 0u32);
        let traces = use_resource(move || async move {
            let _ = refresh(); // subscribe
            fetch_traces().await
        });

        rsx! {
            div { style: "padding:1rem; display:flex; flex-direction:column; gap:0.75rem; height:100%; overflow:auto;",
                div { style: "display:flex; align-items:center; gap:0.75rem;",
                    h2 { style: "margin:0; color:var(--text);", "Traces" }
                    button {
                        style: "padding:0.35rem 0.8rem; border-radius:6px; border:1px solid var(--border); background:var(--surface); color:var(--text); cursor:pointer;",
                        onclick: move |_| refresh += 1,
                        "Refresh"
                    }
                    span { style: "color:var(--text2); font-size:0.85rem;",
                        "OTLP spans from /localhost/nfd/observability"
                    }
                }
                match &*traces.read_unchecked() {
                    None => rsx! { div { style: "color:var(--text2);", "Loading spans…" } },
                    Some(Err(e)) => rsx! {
                        div { style: "color:var(--danger, #c33);", "Failed to fetch traces: {e}" }
                    },
                    Some(Ok(groups)) if groups.is_empty() => rsx! {
                        div { style: "color:var(--text2);",
                            "No spans yet. Generate traffic (e.g. the /bench relay) and Refresh."
                        }
                    },
                    Some(Ok(groups)) => rsx! {
                        for g in groups.iter() {
                            TraceCard { group: g.clone() }
                        }
                    },
                }
            }
        }
    }

    #[component]
    fn TraceCard(group: TraceGroup) -> Element {
        let span_ns = group.max_end.saturating_sub(group.min_start).max(1);
        rsx! {
            div { style: "border:1px solid var(--border); border-radius:8px; padding:0.75rem; background:var(--surface);",
                div { style: "display:flex; justify-content:space-between; margin-bottom:0.5rem;",
                    span { style: "font-family:monospace; color:var(--accent, #58a6ff);", "trace {group.trace_hex}" }
                    span { style: "color:var(--text2); font-size:0.85rem;",
                        "{group.spans.len()} spans · {fmt_dur(span_ns)}"
                    }
                }
                for s in group.spans.iter() {
                    {
                        let off = (s.start_unix_nano.saturating_sub(group.min_start)) as f64 / span_ns as f64 * 100.0;
                        let w = (s.duration_nanos().max(1)) as f64 / span_ns as f64 * 100.0;
                        let is_radio = s.name.starts_with("decide") || s.name.starts_with("actuate") || s.name.starts_with("apply");
                        let bar = if is_radio { "var(--warning, #d29922)" } else { "var(--accent, #58a6ff)" };
                        let attrs = s.attrs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ");
                        rsx! {
                            div { style: "display:flex; align-items:center; gap:0.5rem; margin:0.15rem 0;",
                                span { style: "width:11rem; font-size:0.8rem; color:var(--text); white-space:nowrap; overflow:hidden; text-overflow:ellipsis;",
                                    title: "{attrs}",
                                    "{s.name}"
                                }
                                div { style: "flex:1; height:14px; position:relative; background:var(--bg2, #0d1117); border-radius:3px;",
                                    div { style: "position:absolute; left:{off}%; width:{w}%; min-width:2px; height:100%; background:{bar}; border-radius:3px;" }
                                }
                                span { style: "width:5rem; text-align:right; font-size:0.75rem; color:var(--text2);",
                                    "{fmt_dur(s.duration_nanos())}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
