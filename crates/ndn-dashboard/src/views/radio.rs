use crate::app::AppCtx;
use crate::types::FaceInfo;
use dioxus::prelude::*;

/// The radio-cognition + hardware view models and the `[named-radio]`
/// control-surface parser now live in the shared `ndn-viz` crate; this
/// view consumes them. See `ndn_viz::control`.

/// Web build: fetch the cognition surface and render the decided plan.
#[cfg(feature = "web")]
#[component]
pub fn CognitionPanel() -> Element {
    use crate::app_web::default_ws_url;
    use crate::ws_mgmt::WsMgmtClient;
    use ndn_config::ControlResponse;
    use ndn_packet::Name;
    use ndn_viz::RadioCognition;

    let mut refresh = use_signal(|| 0u32);
    let cog = use_resource(move || async move {
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
        // ext/list is a ControlResponse; the rendered text is its status_text.
        let cr = ControlResponse::decode(content).map_err(|e| format!("decode: {e:?}"))?;
        Ok::<RadioCognition, String>(RadioCognition::parse(&cr.status_text))
    });

    rsx! {
        div { class: "section",
            div { style: "display:flex;align-items:center;gap:0.75rem;",
                div { class: "section-title", style: "margin:0;", "Radio Cognition — Decided Plan" }
                button {
                    style: "padding:0.3rem 0.7rem;border-radius:6px;border:1px solid var(--border);background:var(--surface2);color:var(--text);cursor:pointer;font-size:12px;",
                    onclick: move |_| refresh += 1,
                    "Refresh"
                }
            }
            match &*cog.read_unchecked() {
                None => rsx! { div { class: "empty", "Loading cognition telemetry…" } },
                Some(Err(e)) => rsx! { div { class: "empty", "Cognition surface unavailable: {e}" } },
                Some(Ok(c)) if !c.present => rsx! {
                    div { class: "empty", "No named-radio cognition surface registered on this forwarder." }
                },
                Some(Ok(c)) => rsx! {
                    div { style: "display:flex;flex-wrap:wrap;gap:12px;margin:8px 0;",
                        CogStat { label: "Strategy", value: c.strategy.clone() }
                        CogStat { label: "Managed objects", value: c.managed_objects.clone() }
                        CogStat { label: "Suppressed", value: c.suppressed.clone() }
                        CogStat { label: "Objective", value: c.objective.clone() }
                        CogStat { label: "EDCCA ignored", value: c.contention_edcca_ignored.clone() }
                        CogStat { label: "Defer clamped", value: c.contention_defer_threshold_clamped.clone() }
                        CogStat { label: "FEC parity over-gen", value: c.contention_fec_parity_over_generation.clone() }
                    }
                    div { style: "font-size:11px;color:var(--text-muted);margin:-4px 0 8px;",
                        "Actuator-side contention ledger (cumulative since boot): "
                        "EDCCA ignored = medium claims that bypassed this node's defer policy; "
                        "Defer clamped = defer thresholds pushed outside the decidable band; "
                        "FEC parity over-gen = link-FEC parity budget exceeded."
                    }
                    if c.radios.is_empty() {
                        div { style: "font-size:12px;color:var(--text-muted);",
                            "No per-radio allocation right now — cognition is suppressing TX (no demand/rank). Drive traffic (e.g. the /bench relay) to see decided channel/rate."
                        }
                    } else {
                        table {
                            thead {
                                tr {
                                    th { "Radio" }
                                    th { "Channel" }
                                    th { "MCS" }
                                    th { "NSS" }
                                    th { "BW" }
                                    th { "TX power" }
                                    th { "Link-FEC" }
                                    th { "EDCCA ignore" }
                                    th { "Defer dBm" }
                                    th { "Objective" }
                                    th { "Suppress" }
                                }
                            }
                            tbody {
                                for r in c.radios.iter() {
                                    tr {
                                        td { class: "mono", "{r.id}" }
                                        td { class: "mono", "{r.channel}" }
                                        td { class: "mono", "{r.mcs}" }
                                        td { class: "mono", "{r.nss}" }
                                        td { class: "mono", "{r.bw}" }
                                        td { class: "mono", "{r.tx_power}" }
                                        td { class: "mono", "{r.link_fec}" }
                                        td { class: "mono", "{r.edcca_ignore}" }
                                        td { class: "mono", "{r.defer_threshold_dbm}" }
                                        td { class: "mono", "{r.objective}" }
                                        td { class: "mono", "{r.suppress}" }
                                    }
                                }
                            }
                        }
                    }
                    div { style: "font-size:11px;color:var(--text-muted);margin-top:8px;",
                        "The named-data radio actuates these itself via cognition — read-only. Source: "
                        span { class: "mono", "/localhost/nfd/ext/list" }
                        " [named-radio]."
                    }
                },
            }
        }
    }
}

/// Desktop build: cognition fetch uses the native mgmt path (not yet wired);
/// show a hint rather than the panel.
#[cfg(not(feature = "web"))]
#[component]
pub fn CognitionPanel() -> Element {
    rsx! {
        div { class: "section",
            div { class: "section-title", "Radio Cognition — Decided Plan" }
            div { class: "empty",
                "Cognition telemetry (ext/list) is shown in the web build."
            }
        }
    }
}

#[cfg(feature = "web")]
#[component]
fn CogStat(label: String, value: String) -> Element {
    let shown = if value.is_empty() {
        "—".to_string()
    } else {
        value
    };
    rsx! {
        div { style: "background:var(--surface2);border:1px solid var(--border);border-radius:6px;padding:10px 14px;min-width:120px;",
            div { style: "font-size:11px;color:var(--text-muted);", "{label}" }
            div { style: "font-size:15px;color:var(--text);", "{shown}" }
        }
    }
}

/// Web build: the radio HARDWARE substrate (device capability + PHY health that
/// cognition acts on), from the same `[named-radio]` control surface. Distinct
/// from the decided plan above — read-only ground truth.
#[cfg(feature = "web")]
#[component]
pub fn HardwarePanel() -> Element {
    use crate::app_web::default_ws_url;
    use crate::ws_mgmt::WsMgmtClient;
    use ndn_config::ControlResponse;
    use ndn_packet::Name;
    use ndn_viz::HardwareInventory;

    let mut refresh = use_signal(|| 0u32);
    let hw = use_resource(move || async move {
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
        Ok::<HardwareInventory, String>(HardwareInventory::parse(&cr.status_text))
    });

    let cell = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".to_string());

    rsx! {
        div { class: "section",
            div { style: "display:flex;align-items:center;gap:0.75rem;",
                div { class: "section-title", style: "margin:0;", "Radio Hardware — Substrate" }
                button {
                    style: "padding:0.3rem 0.7rem;border-radius:6px;border:1px solid var(--border);background:var(--surface2);color:var(--text);cursor:pointer;font-size:12px;",
                    onclick: move |_| refresh += 1,
                    "Refresh"
                }
            }
            match &*hw.read_unchecked() {
                None => rsx! { div { class: "empty", "Loading hardware telemetry…" } },
                Some(Err(e)) => rsx! { div { class: "empty", "Hardware surface unavailable: {e}" } },
                Some(Ok(h)) if h.radios.is_empty() => rsx! {
                    div { class: "empty", "No radios registered on this forwarder's cognition surface." }
                },
                Some(Ok(h)) if !h.any_substrate() => rsx! {
                    div { style: "font-size:12px;color:var(--text-muted);",
                        "Radios are present, but this forwarder is not advertising substrate detail yet "
                        "(chip/capability/RSSI/occupancy). Update the named-radio surface to populate this view."
                    }
                },
                Some(Ok(h)) => rsx! {
                    table {
                        thead {
                            tr {
                                th { "Radio" }
                                th { "Chip/class" }
                                th { "Band" }
                                th { "Max MCS" }
                                th { "RX chains" }
                                th { "dBm max" }
                                th { "RSSI" }
                                th { "Occupancy" }
                            }
                        }
                        tbody {
                            for d in h.radios.iter() {
                                tr {
                                    td { class: "mono", "{d.id}" }
                                    td { class: "mono", "{cell(&d.chip)}" }
                                    td { class: "mono", "{cell(&d.band)}" }
                                    td { class: "mono", "{cell(&d.max_mcs)}" }
                                    td { class: "mono", "{cell(&d.rx_chains)}" }
                                    td { class: "mono", "{cell(&d.dbm_max)}" }
                                    td { class: "mono", "{cell(&d.rssi_dbm)}" }
                                    td { class: "mono", "{cell(&d.occupancy_pct)}" }
                                }
                            }
                        }
                    }
                    div { style: "font-size:11px;color:var(--text-muted);margin-top:8px;",
                        "The substrate cognition acts on. Source: "
                        span { class: "mono", "/localhost/nfd/ext/list" }
                        " [named-radio] hardware keys."
                    }
                },
            }
        }
    }
}

#[cfg(not(feature = "web"))]
#[component]
pub fn HardwarePanel() -> Element {
    rsx! {
        div { class: "section",
            div { class: "section-title", "Radio Hardware — Substrate" }
            div { class: "empty", "Hardware substrate (ext/list) is shown in the web build." }
        }
    }
}

#[component]
pub fn Radio() -> Element {
    let ctx = use_context::<AppCtx>();
    let faces = ctx.faces.read();
    let counters = ctx.counters.read();

    // Show wireless-relevant faces: ether-multicast, WFB (wifibroadcast), multicast UDP
    let wireless_faces: Vec<&FaceInfo> = faces
        .iter()
        .filter(|f| {
            let uri = f.remote_uri.as_deref().unwrap_or("");
            let kind = f.kind.as_deref().unwrap_or("");
            uri.starts_with("ether://")
                || kind.eq_ignore_ascii_case("wfb")
                || kind.eq_ignore_ascii_case("EtherMulticast")
                || uri.starts_with("ether-multicast://")
        })
        .collect();

    rsx! {
        CognitionPanel {}
        HardwarePanel {}
        div { class: "section",
            div { class: "section-title", "Radio / Wireless Faces" }
            if wireless_faces.is_empty() {
                div { class: "empty",
                    "No wireless faces detected. Radio view is relevant when Ethernet multicast or WFB (Wifibroadcast) faces are active."
                }
            } else {
                table {
                    thead {
                        tr {
                            th { "Face" }
                            th { "Kind" }
                            th { "URI" }
                            th { "In ↓ Interests" }
                            th { "In ↓ Data" }
                            th { "Out ↑ Interests" }
                            th { "Out ↑ Data" }
                            th { "Quality Est." }
                        }
                    }
                    tbody {
                        for face in &wireless_faces {
                            {
                                let fid = face.face_id;
                                let counter = counters.iter().find(|c| c.face_id == fid);
                                let (in_i, in_d, out_i, out_d) = match counter {
                                    Some(c) => (c.in_interests, c.in_data, c.out_interests, c.out_data),
                                    None    => (0, 0, 0, 0),
                                };
                                // Estimate link quality from data satisfaction: in_data / in_interests
                                let quality_pct = if in_i > 0 {
                                    ((in_d as f64 / in_i as f64) * 100.0).min(100.0)
                                } else {
                                    0.0
                                };
                                let quality_class = if quality_pct >= 80.0 {
                                    "badge badge-green"
                                } else if quality_pct >= 50.0 {
                                    "badge badge-yellow"
                                } else if in_i == 0 {
                                    "badge badge-gray"
                                } else {
                                    "badge badge-red"
                                };
                                rsx! {
                                    tr {
                                        td { class: "mono", "{fid}" }
                                        td {
                                            span { class: "{face.kind_badge_class()}", "{face.kind_label()}" }
                                        }
                                        td { class: "mono", style: "font-size:11px;",
                                            "{face.remote_uri.as_deref().unwrap_or(\"—\")}"
                                        }
                                        td { class: "mono", "{in_i}" }
                                        td { class: "mono", "{in_d}" }
                                        td { class: "mono", "{out_i}" }
                                        td { class: "mono", "{out_d}" }
                                        td {
                                            span { class: "{quality_class}",
                                                if in_i == 0 {
                                                    "—"
                                                } else {
                                                    "{quality_pct:.0}%"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { style: "font-size:11px;color:var(--text-muted);margin-top:8px;",
                    "Quality estimate = Data satisfied / Interests received (from face counters). "
                    "Full RSSI and SNR data requires "
                    span { class: "mono", "faces/link-quality" }
                    " (Phase 4.3)."
                }
            }
        }

        div { class: "section",
            div { class: "section-title", "All Faces — Link Overview" }
            if counters.is_empty() {
                div { class: "empty", "No counter data available." }
            } else {
                div { style: "display:flex;flex-wrap:wrap;gap:12px;",
                    for c in counters.iter() {
                        {
                            let face_info = faces.iter().find(|f| f.face_id == c.face_id);
                            let kind = face_info.map(|f| f.kind_label()).unwrap_or("?");
                            let total_in = c.in_interests + c.in_data;
                            let total_out = c.out_interests + c.out_data;
                            rsx! {
                                div { style: "background:var(--surface2);border:1px solid var(--border);border-radius:6px;padding:12px;min-width:160px;",
                                    div { style: "font-size:11px;color:var(--text-muted);margin-bottom:6px;",
                                        "Face "
                                        span { class: "mono", "{c.face_id}" }
                                        " · "
                                        span { class: "badge badge-gray", "{kind}" }
                                    }
                                    div { style: "font-size:12px;color:var(--accent);", "↓ {total_in} pkts" }
                                    div { style: "font-size:12px;color:var(--green);", "↑ {total_out} pkts" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
