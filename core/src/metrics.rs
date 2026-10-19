//! Prometheus metrics definitions for Perigee.
//!
//! All metrics live in one place and are registered into a single
//! [`Registry`].  Both the HTTP middleware layer and the RPC provider receive
//! an `Arc<Metrics>` so they can record observations without touching
//! `AppState`.
//!
//! ## Metrics exposed
//!
//! | Name | Type | Labels | Description |
//! |------|------|--------|-------------|
//! | `http_requests_total` | Counter | `method`, `route`, `status` | Every completed HTTP request |
//! | `http_request_duration_seconds` | Histogram | `method`, `route` | End-to-end request latency |
//! | `http_requests_in_flight` | Gauge | `method`, `route` | Currently-active requests |
//! | `rpc_calls_total` | Counter | `provider`, `method`, `status` | Every RPC call, labelled success/error |
//! | `rpc_call_duration_seconds` | Histogram | `provider`, `method` | RPC round-trip latency |
//! | `rpc_circuit_breaker_tripped` | Gauge | `provider` | 1 when circuit-breaker is open, 0 when closed |
//! | `simulation_requests_total` | Counter | `endpoint`, `cache_status` | Simulation requests by cache outcome |
//! | `simulation_latency_seconds` | Histogram | `endpoint` | Simulation-specific latency |
//! | `resource_utilization_percent` | Gauge | `resource` | Latest resource-utilisation sample |
//! | `perigee_shielded_rail_fallback_total` | Counter | — | Settlements that fell back from the shielded tokenless rail to the transparent rail (BE-028) |
//! | `security_audit_events_total` | Counter | `event`, `status` | Security audit events and monitoring (Issue #398) |

use prometheus::{
    opts, register_gauge_vec_with_registry, register_histogram_vec_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry, GaugeVec,
    HistogramOpts, HistogramVec, IntCounter, IntCounterVec, Registry,
};
use std::sync::Arc;

/// Latency buckets in seconds — fine-grained up to 5 s, then coarser.
const HTTP_LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Latency buckets for RPC calls (network round-trips, typically faster or
/// much slower depending on congestion).
const RPC_LATENCY_BUCKETS: &[f64] = &[
    0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];

/// Central metrics store shared by HTTP middleware and the RPC layer.
#[derive(Clone)]
pub struct Metrics {
    /// Process-wide Prometheus registry (all collectors below are registered
    /// here and nowhere else).
    pub registry: Registry,

    // ── HTTP ─────────────────────────────────────────────────────────────
    /// Total requests completed, by method × route × HTTP status code.
    pub http_requests_total: IntCounterVec,
    /// End-to-end request latency distribution, by method × route.
    pub http_request_duration_seconds: HistogramVec,
    /// Gauge of currently in-flight requests, by method × route.
    pub http_requests_in_flight: GaugeVec,

    // ── RPC ──────────────────────────────────────────────────────────────
    /// Total RPC calls, by provider name × JSON-RPC method × outcome
    /// (`success` | `error` | `timeout`).
    pub rpc_calls_total: IntCounterVec,
    /// RPC call round-trip latency, by provider name × JSON-RPC method.
    pub rpc_call_duration_seconds: HistogramVec,
    /// 1 when the circuit-breaker for the named provider is open; 0 when closed.
    pub rpc_circuit_breaker_tripped: GaugeVec,

    // ── Simulation (kept for backward compat with existing handler code) ──
    /// Simulation requests by endpoint × cache outcome (`HIT` | `MISS` | `LOCAL`).
    pub simulation_requests_total: IntCounterVec,
    /// Simulation latency by endpoint.
    pub simulation_latency_seconds: HistogramVec,
    /// Latest resource-utilisation sample (e.g. efficiency_score) by resource name.
    pub resource_utilization_percent: GaugeVec,

    // ── Shielded-rail settlement (BE-028) ────────────────────────────────
    /// Number of settlements that fell back from the shielded tokenless rail
    /// to the transparent rail (shielded rail unavailable or timed out).
    pub shielded_rail_fallback_total: IntCounter,

    // ── Security Audit & Monitoring (Issue #398) ─────────────────────────
    /// Total security audit events recorded, labelled by event type and status.
    pub security_audit_events_total: IntCounterVec,
}

impl Metrics {
    /// Construct and register all collectors into a fresh private [`Registry`].
    ///
    /// Returns an error if any registration fails (which would indicate a
    /// programming mistake — duplicate metric names).
    pub fn new() -> Result<Arc<Self>, prometheus::Error> {
        let registry = Registry::new();

        // ── HTTP ─────────────────────────────────────────────────────────
        let http_requests_total = register_int_counter_vec_with_registry!(
            opts!(
                "http_requests_total",
                "Total HTTP requests completed, labelled by method, route, and status code."
            ),
            &["method", "route", "status"],
            registry
        )?;

        let http_request_duration_seconds = register_histogram_vec_with_registry!(
            HistogramOpts::new(
                "http_request_duration_seconds",
                "End-to-end HTTP request latency in seconds.",
            )
            .buckets(HTTP_LATENCY_BUCKETS.to_vec()),
            &["method", "route"],
            registry
        )?;

        let http_requests_in_flight = register_gauge_vec_with_registry!(
            opts!(
                "http_requests_in_flight",
                "Number of HTTP requests currently being processed."
            ),
            &["method", "route"],
            registry
        )?;

        // ── RPC ──────────────────────────────────────────────────────────
        let rpc_calls_total = register_int_counter_vec_with_registry!(
            opts!(
                "rpc_calls_total",
                "Total Stellar RPC calls, labelled by provider, JSON-RPC method, and outcome."
            ),
            &["provider", "method", "status"],
            registry
        )?;

        let rpc_call_duration_seconds = register_histogram_vec_with_registry!(
            HistogramOpts::new(
                "rpc_call_duration_seconds",
                "Stellar RPC call round-trip latency in seconds.",
            )
            .buckets(RPC_LATENCY_BUCKETS.to_vec()),
            &["provider", "method"],
            registry
        )?;

        let rpc_circuit_breaker_tripped = register_gauge_vec_with_registry!(
            opts!(
                "rpc_circuit_breaker_tripped",
                "1 when a provider's circuit-breaker is open (tripped), 0 when closed."
            ),
            &["provider"],
            registry
        )?;

        // ── Simulation ───────────────────────────────────────────────────
        let simulation_requests_total = register_int_counter_vec_with_registry!(
            opts!(
                "simulation_requests_total",
                "Total simulation requests by endpoint and cache status."
            ),
            &["endpoint", "cache_status"],
            registry
        )?;

        let simulation_latency_seconds = register_histogram_vec_with_registry!(
            HistogramOpts::new(
                "simulation_latency_seconds",
                "Simulation request latency in seconds.",
            )
            .buckets(HTTP_LATENCY_BUCKETS.to_vec()),
            &["endpoint"],
            registry
        )?;

        let resource_utilization_percent = register_gauge_vec_with_registry!(
            opts!(
                "resource_utilization_percent",
                "Resource utilisation percentage from the latest simulation sample."
            ),
            &["resource"],
            registry
        )?;

        let shielded_rail_fallback_total = register_int_counter_with_registry!(
            opts!(
                "perigee_shielded_rail_fallback_total",
                "Number of settlements that fell back from the shielded tokenless rail to the transparent rail."
            ),
            registry
        )?;

        let security_audit_events_total = register_int_counter_vec_with_registry!(
            opts!(
                "security_audit_events_total",
                "Total security-sensitive events recorded, labelled by event and status."
            ),
            &["event", "status"],
            registry
        )?;

        Ok(Arc::new(Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            http_requests_in_flight,
            rpc_calls_total,
            rpc_call_duration_seconds,
            rpc_circuit_breaker_tripped,
            simulation_requests_total,
            simulation_latency_seconds,
            resource_utilization_percent,
            shielded_rail_fallback_total,
            security_audit_events_total,
        }))
    }

    /// Record a completed RPC probe (health-check / getLatestLedger).
    ///
    /// - `provider_name` — human-readable provider label.
    /// - `elapsed_secs`  — round-trip duration.
    /// - `ok`            — whether the probe succeeded.
    pub fn record_rpc_probe(&self, provider_name: &str, elapsed_secs: f64, ok: bool) {
        let status = if ok { "success" } else { "error" };
        self.rpc_calls_total
            .with_label_values(&[provider_name, "getLatestLedger", status])
            .inc();
        self.rpc_call_duration_seconds
            .with_label_values(&[provider_name, "getLatestLedger"])
            .observe(elapsed_secs);
    }

    /// Update the circuit-breaker gauge for the named provider.
    pub fn set_circuit_breaker(&self, provider_name: &str, tripped: bool) {
        self.rpc_circuit_breaker_tripped
            .with_label_values(&[provider_name])
            .set(if tripped { 1.0 } else { 0.0 });
    }

    /// Record one settlement that fell back from the shielded tokenless rail
    /// to the transparent rail (BE-028).
    pub fn record_shielded_rail_fallback(&self) {
        self.shielded_rail_fallback_total.inc();
    }
}
