use std::net::{IpAddr, Ipv4Addr};
use std::os::fd::AsFd;
use std::sync::{Arc, Mutex};

use actix_web::web::Data;
use actix_web::{get, middleware, App, HttpRequest, HttpResponse, HttpServer, Responder};
use anyhow::Context;
use aya::maps::{HashMap, Map, MapData, RingBuf};
use aya::programs::{Xdp, XdpFlags};
use aya::{include_bytes_aligned, Bpf};
use aya_log::BpfLogger;
use clap::Parser;
use iface::get_ifaces;
use kubernetes::VipEvent;
use lb_inter_node_exporter_common::Ipv4Event;
use log::{debug, info, warn};
use prometheus::{Encoder, TextEncoder};
use tokio::signal;
use tokio::sync::mpsc::unbounded_channel;

use crate::error::Error;
use crate::kubernetes::ServiceWatcher;
use crate::trace::Metrics;

mod error;
mod iface;
mod kubernetes;
mod trace;

#[derive(Debug, Parser)]
struct Cmd {
    #[clap(short = 'i', long, default_value = "eth0")]
    iface: Vec<String>,

    #[clap(long = "log-level", default_value = "info")]
    log_level: String,

    #[clap(long = "metrics-endpoint", default_value = "http://localhost:61678")]
    metrics_endpoint: String,

    #[clap(short = 'p', long = "port", default_value = "8080")]
    port: u32,

    #[clap(
        long = "xdp-mode",
        default_value = "",
        help = "XDP mode(native, hw, skb)"
    )]
    xdp_mode: String,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    registry: prometheus::Registry,
}

impl State {
    pub fn metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cmd = Cmd::parse();

    trace::prepare_tracing(&cmd.log_level, &cmd.metrics_endpoint);

    let target_ifaces = get_ifaces(&cmd.iface).await.unwrap();

    // Bump the memlock rlimit. This is needed for older kernels that don't use the
    // new memcg based accounting, see https://lwn.net/Articles/837122/
    let rlim = libc::rlimit {
        rlim_cur: libc::RLIM_INFINITY,
        rlim_max: libc::RLIM_INFINITY,
    };
    let ret = unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlim) };
    if ret != 0 {
        debug!("remove limit on locked memory failed, ret is: {}", ret);
    }

    // This will include your eBPF object file as raw bytes at compile-time and load it at
    // runtime. This approach is recommended for most real-world use cases. If you would
    // like to specify the eBPF program at runtime rather than at compile-time, you can
    // reach for `Bpf::load_file` instead.

    #[cfg(debug_assertions)]
    let mut bpf = Bpf::load(include_bytes_aligned!(
        "../../target/bpfel-unknown-none/debug/lb-inter-node-exporter"
    ))?;
    #[cfg(not(debug_assertions))]
    let mut bpf = Bpf::load(include_bytes_aligned!(
        "../../target/bpfel-unknown-none/release/lb-inter-node-exporter"
    ))?;
    if let Err(e) = BpfLogger::init(&mut bpf) {
        // This can happen if you remove all log statements from your eBPF program.
        warn!("failed to initialize eBPF logger: {}", e);
    }
    let program: &mut Xdp = bpf
        .program_mut("lb_inter_node_exporter")
        .unwrap()
        .try_into()?;
    program.load()?;
    let xdp_flag = get_xdp_mode(&cmd.xdp_mode);
    for iface in target_ifaces.iter() {
        program.attach(&iface.name, xdp_flag)
            .context("failed to attach the XDP program with default flags - try changing XdpFlags::default() to XdpFlags::SKB_MODE")?;
        tracing::info!(
            ifname = iface.name,
            ifindex = iface.index,
            "Attach the XDP program"
        );
    }

    let mut ipv4_vips = HashMap::try_from(bpf.take_map("IPV4VIP").expect("failed to get IPV4VIP"))?;
    let mut ipv4_events =
        RingBuf::try_from(bpf.take_map("IPV4EVENT").expect("failed to get IPV4EVENT"))?;

    let state = State::default();

    let (event_send, mut event_recv) = unbounded_channel();

    tokio::spawn(async move {
        let mut svc_map = std::collections::HashMap::<(String, String), IpAddr>::new();

        while let Some(event) = event_recv.recv().await {
            // poll vip events
            match event {
                VipEvent::Add(lb) => {
                    // add Vip
                    tracing::info!(
                        name = lb.name,
                        namespace = lb.namespace,
                        vip =? lb.addr,
                        "Add to track VIP"
                    );
                    svc_map.insert((lb.name.clone(), lb.namespace.clone()), lb.addr.unwrap());
                    match lb.addr.unwrap() {
                        IpAddr::V4(addr) => {
                            let addr_num: u32 = u32::from(addr);
                            ipv4_vips.insert(addr_num, 0, 0);
                        }
                        IpAddr::V6(_addr) => {
                            // not implemented
                        }
                    }
                }
                VipEvent::Delete(lb) => {
                    // delete Vip
                    tracing::info!(
                        name = lb.name,
                        namespace = lb.namespace,
                        vip =? lb.addr,
                        "Delete the tracking VIP"
                    );
                    if let Some(addr) = svc_map.remove(&(lb.name.clone(), lb.namespace.clone())) {
                        match addr {
                            IpAddr::V4(addr) => {
                                let addr_num: u32 = u32::from(addr);
                                ipv4_vips.remove(&addr_num);
                            }
                            IpAddr::V6(_addr) => {
                                // not implemented
                            }
                        }
                    }
                }
            }
        }
    });

    let svc_watcher = ServiceWatcher::new(event_send.clone()).await;

    tokio::spawn(async move {
        svc_watcher.run().await.expect("Got error");
    });

    let metrics_collector = Metrics::default().register(&state.registry).unwrap();

    tokio::spawn(async move {
        loop {
            if let Some(event) = ipv4_events.next() {
                let ipv4_event: Ipv4Event = (*event).into();
                let src_addr = u32_to_addr(ipv4_event.src_addr);
                let dst_addr = u32_to_addr(ipv4_event.dst_addr);
                tracing::info!(src_addr=?src_addr, dst_addr=?dst_addr, src_port = ipv4_event.src_port, dst_port = ipv4_event.dst_port, "Received by intermediate node");
                metrics_collector.picked_total(IpAddr::V4(src_addr), IpAddr::V4(dst_addr));
            }
        }
    });

    let server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(state.clone()))
            .service(health)
            .service(ready)
            .service(metrics)
            .wrap(
                middleware::Logger::default()
                    .exclude("/healthz")
                    .exclude("/readyz"),
            )
    })
    .bind(format!("0.0.0.0:{}", cmd.port))
    .unwrap()
    .shutdown_timeout(5);

    server.run().await.unwrap();

    Ok(())
}

#[get("/healthz")]
async fn health(_: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/readyz")]
async fn ready(_: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json("ready")
}

#[get("/metrics")]
async fn metrics(c: Data<State>, _req: HttpRequest) -> impl Responder {
    let metrics = c.metrics();
    let encoder = TextEncoder::new();
    let mut buffer = vec![];
    encoder.encode(&metrics, &mut buffer).unwrap();
    HttpResponse::Ok().body(buffer)
}

fn get_xdp_mode(mode: &str) -> XdpFlags {
    match mode.to_lowercase().as_str() {
        "native" => XdpFlags::DRV_MODE,
        "hw" => XdpFlags::HW_MODE,
        "skb" => XdpFlags::SKB_MODE,
        _ => XdpFlags::default(),
    }
}

fn u32_to_addr(x: u32) -> Ipv4Addr {
    let b1: u8 = ((x >> 24) & 0xff) as u8;
    let b2: u8 = ((x >> 16) & 0xff) as u8;
    let b3: u8 = ((x >> 8) & 0xff) as u8;
    let b4: u8 = (x & 0xff) as u8;
    Ipv4Addr::new(b1, b2, b3, b4)
}
