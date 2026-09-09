//! **Monitors** — the node-level long-running watchers (uptime, forwarding
//! soak, per-radio link quality) as reported by the forwarder's `[monitors]`
//! control surface. Read-only, unsigned: the same `ext/list` dataset as the
//! radio cognition view, projected through `ndn_viz::Monitors`.

#[cfg(feature = "web")]
pub use web::Monitors;

#[cfg(not(feature = "web"))]
#[dioxus::prelude::component]
pub fn Monitors() -> dioxus::prelude::Element {
    use dioxus::prelude::*;
    rsx! {
        div { class: "placeholder", style: "padding:2rem;color:var(--text2);",
            "The monitors view is available in the web build."
        }
    }
}

#[cfg(feature = "web")]
mod web {
    use dioxus::prelude::*;

    use crate::app_web::default_ws_url;
    use crate::ws_mgmt::WsMgmtClient;
    use ndn_config::ControlResponse;
    use ndn_packet::Name;
    use ndn_viz::Monitors as MonitorModel;

    fn fmt_duration(s: &str) -> String {
        let Ok(secs) = s.parse::<u64>() else {
            return s.to_string();
        };
        format!(
            "{}h{:02}m{:02}s",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    }

    #[component]
    pub fn Monitors() -> Element {
        let mut refresh = use_signal(|| 0u32);
        let mons = use_resource(move || async move {
            let _ = refresh();
            let mut client = WsMgmtClient::new(&default_ws_url());
            client.connect().await.map_err(|e| e.to_string())?;
            let name = Name::root()
                .append(b"localhost")
                .append(b"nfd")
                .append(b"ext")
                .append(b"list");
            let content = client
                .fetch_data(name)
                .await
                .map_err(|e| format!("ext/list: {e}"))?;
            let cr = ControlResponse::decode(content).map_err(|e| format!("decode: {e:?}"))?;
            Ok::<MonitorModel, String>(MonitorModel::parse(&cr.status_text))
        });

        rsx! {
            div { class: "section",
                div { style: "display:flex;align-items:center;gap:0.75rem;",
                    div { class: "section-title", style: "margin:0;", "Monitors — Long-Running Watchers" }
                    button {
                        style: "padding:0.3rem 0.7rem;border-radius:6px;border:1px solid var(--border);background:var(--surface2);color:var(--text);cursor:pointer;font-size:12px;",
                        onclick: move |_| refresh += 1,
                        "Refresh"
                    }
                }
                match &*mons.read_unchecked() {
                    None => rsx! { div { class: "empty", "Loading monitors…" } },
                    Some(Err(e)) => rsx! { div { class: "empty", "Monitors surface unavailable: {e}" } },
                    Some(Ok(m)) if !m.present => rsx! {
                        div { class: "empty", "No `[monitors]` control surface registered on this forwarder." }
                    },
                    Some(Ok(m)) => rsx! {
                        if m.monitors.is_empty() {
                            div { style: "font-size:12px;color:var(--text-muted);",
                                "The monitors registry is empty — no watchers started."
                            }
                        } else {
                            table {
                                thead {
                                    tr {
                                        th { "Watcher" }
                                        th { "Kind" }
                                        th { "Status" }
                                        th { "Detail" }
                                        th { "Duration" }
                                        th { "OK" }
                                        th { "Fail" }
                                    }
                                }
                                tbody {
                                    for m in m.monitors.iter() {
                                        tr {
                                            td { class: "mono", "{m.id}" }
                                            td { class: "mono", "{m.kind}" }
                                            td { class: "mono", "{m.status}" }
                                            td { style: "font-size:12px;color:var(--text-muted);", "{m.detail}" }
                                            td { class: "mono", "{fmt_duration(&m.duration_s)}" }
                                            td { class: "mono", "{m.ok}" }
                                            td { class: "mono", "{m.fail}" }
                                        }
                                    }
                                }
                            }
                        }
                        div { style: "font-size:11px;color:var(--text-muted);margin-top:8px;",
                            "ok/fail are cumulative tick counts since the watcher started. Source: "
                            span { class: "mono", "/localhost/nfd/ext/list" }
                            " `[monitors]`."
                        }
                    },
                }
            }
        }
    }
}
