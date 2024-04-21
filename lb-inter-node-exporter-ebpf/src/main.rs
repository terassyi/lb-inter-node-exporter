#![no_std]
#![no_main]

use core::mem;

use aya_ebpf::{
    bindings::xdp_action,
    macros::{map, xdp},
    maps::{HashMap, RingBuf},
    programs::XdpContext,
};
use aya_log_ebpf::info;
use lb_inter_node_exporter_common::Ipv4Event;
use network_types::{
    eth::{EthHdr, EtherType},
    ip::{IpHdr, IpProto, Ipv4Hdr, Ipv6Hdr},
    tcp::TcpHdr,
};

#[map]
static IPV4VIP: HashMap<u32, u32> = HashMap::with_max_entries(1024, 0);
#[map]
static IPV6VIP: HashMap<u128, u32> = HashMap::with_max_entries(1024, 0);
#[map]
static IPV4EVENT: RingBuf = RingBuf::with_byte_size(1024 * 1024, 0);
#[map]
static IPV6EVENT: RingBuf = RingBuf::with_byte_size(1024 * 1024, 0);

#[inline(always)]
unsafe fn ptr_at<T>(ctx: &XdpContext, offset: usize) -> Result<*const T, ()> {
    let start = ctx.data();
    let end = ctx.data_end();
    let len = mem::size_of::<T>();

    if start + offset + len > end {
        return Err(());
    }

    let ptr = (start + offset) as *const T;
    Ok(&*ptr)
}

#[xdp]
pub fn lb_inter_node_exporter(ctx: XdpContext) -> u32 {
    match try_lb_inter_node_exporter(ctx) {
        Ok(ret) => ret,
        Err(_) => xdp_action::XDP_ABORTED,
    }
}

fn is_ipv4vip(addr: u32) -> bool {
    unsafe { IPV4VIP.get(&addr).is_some() }
}

fn is_ipv6vip(addr: u128) -> bool {
    unsafe { IPV6VIP.get(&addr).is_some() }
}

fn try_lb_inter_node_exporter(ctx: XdpContext) -> Result<u32, ()> {
    // info!(&ctx, "received a packet");
    let ethhdr: *const EthHdr = unsafe { ptr_at(&ctx, 0)? };
    match unsafe { (*ethhdr).ether_type } {
        EtherType::Ipv4 => {
            let ipv4hdr: *const Ipv4Hdr = unsafe { ptr_at(&ctx, EthHdr::LEN)? };
            let dst_addr = u32::from_be(unsafe { (*ipv4hdr).dst_addr });
            if !is_ipv4vip(dst_addr) {
                return Ok(xdp_action::XDP_PASS);
            }

            let ip_hdr_len = unsafe { (*ipv4hdr).ihl() * 4 } as usize;
            match unsafe { (*ipv4hdr).proto } {
                IpProto::Tcp => {}
                _ => return Ok(xdp_action::XDP_PASS),
            };

            let tcphdr: *const TcpHdr = unsafe { ptr_at(&ctx, EthHdr::LEN + ip_hdr_len)? };
            if unsafe { (*tcphdr).syn() } == 0 {
                return Ok(xdp_action::XDP_PASS);
            }
            let src_port = unsafe { (*tcphdr).source };
            let dst_port = unsafe { (*tcphdr).dest };
            let src_addr = unsafe { (*ipv4hdr).src_addr };
            let mut entry = match IPV4EVENT.reserve::<Ipv4Event>(0) {
                Some(entry) => entry,
                None => return Ok(xdp_action::XDP_PASS),
            };
            let event = Ipv4Event {
                src_addr,
                dst_addr,
                src_port,
                dst_port,
            };
            entry.write(event);
            entry.submit(0);
        }
        EtherType::Ipv6 => {
            // TODO: support ipv6
            // let ipv6hdr: *const Ipv6Hdr = unsafe { ptr_at(&ctx, EthHdr::LEN)? };
            // let dst_addr = unsafe { (*ipv6hdr).dst_addr };
            // if is_ipv6vip(dst_addr.in6_u) {
            //     return Ok(xdp_action::XDP_PASS)
            // }
        }
        _ => return Ok(xdp_action::XDP_PASS),
    }

    Ok(xdp_action::XDP_PASS)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
