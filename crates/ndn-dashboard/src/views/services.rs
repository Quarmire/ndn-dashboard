//! **Services** — the forwarder's service-discovery state: announced and
//! discovered service records, plus the Hello/discovery runtime config.
//! Read-only — the read never mutates (no `announce`/`withdraw`).
//! `service/*` and `discovery/*` are extended mgmt modules, so the read is
//! signed: the view needs the operator signing identity (remote signer)
//! provisioned. Models + parsing live in `ndn-viz` (`ndn_viz::service`);
//! this view owns the transport (WS mgmt link) and rendering.

#[cfg(feature = "web")]
pub use web::Services;

#[cfg(not(feature = "web"))]
#[dioxus::prelude::component]
pub fn Services() -> dioxus::prelude::Element {
    use dioxus::prelude::*;
    rsx! {
        div { class: "placeholder", style: "padding:2rem;color:var(--text2);",
            "The services view is available in the web build."
        }
    }
}

#[cfg(feature = "web")]
mod web {
    use std::collections::HashSet;

    use dioxus::prelude::*;

    use crate::app_web::default_ws_url;
    use crate::ws_mgmt::WsMgmtClient;
    use ndn_viz::service::{
        DiscoveryStatus, ServiceReadError, ServiceRecord, ServiceState, parse_discovery_status,
        parse_service_browse,
    };

    /// Read the service-layer state over the mgmt seam (best-effort per
    /// dataset): a 404 on `service/browse` means discovery is not enabled
    /// (empty set); a failed `service/list` degrades to "no local flag"; a
    /// failed `discovery/status` degrades to `discovery = None`.
    async fn read_service_state(
        client: &mut WsMgmtClient,
    ) -> Result<ServiceState, ServiceReadError> {
        let mut services: Vec<ServiceRecord> =
            match client.send_cmd("service", "browse", None).await {
                Ok(resp) if resp.is_ok() => parse_service_browse(&resp.status_text),
                Ok(resp) if resp.status_code == 404 => Vec::new(),
                Ok(resp) => {
                    return Err(ServiceReadError::Rejected {
                        status: resp.status_code,
                        text: resp.status_text,
                    });
                }
                Err(e) => return Err(ServiceReadError::Transport(e.to_string())),
            };

        let mut local: HashSet<String> = HashSet::new();
        if let Ok(resp) = client.send_cmd("service", "list", None).await
            && resp.is_ok()
        {
            for r in parse_service_browse(&resp.status_text) {
                local.insert(r.prefix);
            }
        }
        for r in &mut services {
            r.local = local.contains(&r.prefix);
        }

        let mut discovery: Option<DiscoveryStatus> = None;
        if let Ok(resp) = client.send_cmd("discovery", "status", None).await
            && resp.is_ok()
        {
            discovery = parse_discovery_status(&resp.status_text);
        }

        Ok(ServiceState {
            discovery,
            services,
        })
    }

    #[component]
    pub fn Services() -> Element {
        let mut refresh = use_signal(|| 0u32);
        let state = use_resource(move || async move {
            let _ = refresh();
            let mut client = WsMgmtClient::new(&default_ws_url());
            client
                .connect()
                .await
                .map_err(|e| ServiceReadError::Transport(e.to_string()))?;
            read_service_state(&mut client).await
        });

        rsx! {
            div { class: "section",
                div { style: "display:flex;align-items:center;gap:0.75rem;",
                    div { class: "section-title", style: "margin:0;", "Services — Discovery" }
                    button {
                        style: "padding:0.3rem 0.7rem;border-radius:6px;border:1px solid var(--border);background:var(--surface2);color:var(--text);cursor:pointer;font-size:12px;",
                        onclick: move |_| refresh += 1,
                        "Refresh"
                    }
                }
                match &*state.read_unchecked() {
                    None => rsx! { div { class: "empty", "Loading services…" } },
                    Some(Err(ServiceReadError::Rejected { status: 401, .. })) => rsx! {
                        div { class: "empty",
                            "Reading the service dataset requires the operator signing identity — "
                            "service/* and discovery/* are signed extended modules. Pair a signer "
                            "(Identity → Pairing), then refresh."
                        }
                    },
                    Some(Err(e)) => rsx! { div { class: "empty", "Service state unavailable: {e}" } },
                    Some(Ok(st)) => rsx! {
                        if let Some(d) = &st.discovery {
                            div { style: "display:flex;flex-wrap:wrap;gap:12px;margin:8px 0;",
                                CogStat { label: "Hello strategy".to_string(), value: d.hello_strategy.clone() }
                                CogStat { label: "Hello interval".to_string(), value: format!("{}–{} ms", d.hello_interval_base_ms, d.hello_interval_max_ms) }
                                CogStat { label: "Liveness".to_string(), value: format!("{} ms × {}", d.liveness_timeout_ms, d.liveness_miss_count) }
                                CogStat { label: "Prefix announcement".to_string(), value: d.prefix_announcement.clone() }
                                CogStat { label: "Auto faces".to_string(), value: if d.auto_create_faces { "on".to_string() } else { "off".to_string() } }
                            }
                        } else {
                            div { class: "empty", style: "margin:8px 0;",
                                "Discovery is not enabled on this forwarder."
                            }
                        }
                        if st.services.is_empty() {
                            div { style: "font-size:12px;color:var(--text-muted);",
                                "No service records known — nothing announced or discovered yet."
                            }
                        } else {
                            table {
                                thead {
                                    tr {
                                        th { "Service prefix" }
                                        th { "Node" }
                                        th { "Freshness" }
                                        th { "Origin" }
                                    }
                                }
                                tbody {
                                    for r in st.services.iter() {
                                        tr {
                                            td { class: "mono", "{r.prefix}" }
                                            td { class: "mono", "{r.node}" }
                                            td { class: "mono", "{r.freshness_ms} ms" }
                                            td {
                                                if r.local {
                                                    span { class: "badge", "local" }
                                                } else {
                                                    span { style: "font-size:12px;color:var(--text-muted);", "discovered" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { style: "font-size:11px;color:var(--text-muted);margin-top:8px;",
                            "Read-only: service discovery records as the forwarder's SD protocol sees them. "
                            "Source: "
                            span { class: "mono", "/localhost/nfd/service/browse" }
                            " + "
                            span { class: "mono", "/localhost/nfd/discovery/status" }
                            "."
                        }
                    },
                }
            }
        }
    }

    /// Small stat tile (same shape as the radio cognition panel's).
    #[component]
    fn CogStat(label: String, value: String) -> Element {
        rsx! {
            div { style: "background:var(--surface2);border:1px solid var(--border);border-radius:6px;padding:10px 14px;min-width:120px;",
                div { style: "font-size:11px;color:var(--text-muted);", "{label}" }
                div { style: "font-size:15px;color:var(--text);", "{value}" }
            }
        }
    }
}
