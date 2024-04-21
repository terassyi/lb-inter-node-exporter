use std::{net::IpAddr, str::FromStr};

use opentelemetry::trace::SpanBuilder;
use opentelemetry_otlp::WithExportConfig;
use prometheus::{opts, IntCounterVec};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};

pub fn prepare_tracing(level: &str, metrics_endpoint: &str) {
    let span = SpanBuilder::default().with_kind(opentelemetry::trace::SpanKind::Internal);

    let a = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(metrics_endpoint),
        )
        .with_trace_config(opentelemetry_sdk::trace::config().with_resource(
            opentelemetry_sdk::Resource::new(vec![opentelemetry::KeyValue::new(
                "service.name",
                "lb-inter-node-exporter",
            )]),
        ))
        .install_simple()
        .unwrap();

    Registry::default()
        .with(tracing_subscriber::fmt::Layer::new().with_ansi(true).json())
        .with(tracing_subscriber::filter::LevelFilter::from_str(level).unwrap())
        .init();
}

pub struct Metrics {
    picked_total: IntCounterVec,
}

impl Default for Metrics {
    fn default() -> Self {
        let picked_total = IntCounterVec::new(
            opts!(
                "lb_inter_node_exporter_picked_total",
                "The count of picked as the intermediate node"
            ),
            &["src", "dst"],
        )
        .unwrap();

        Self { picked_total }
    }
}

impl Metrics {
    pub fn register(self, registry: &prometheus::Registry) -> Result<Self, prometheus::Error> {
        registry.register(Box::new(self.picked_total.clone()))?;
        Ok(self)
    }
    pub fn picked_total(&self, src: IpAddr, dst: IpAddr) {
        self.picked_total
            .with_label_values(&[src.to_string().as_str(), dst.to_string().as_str()])
            .inc();
    }
}
