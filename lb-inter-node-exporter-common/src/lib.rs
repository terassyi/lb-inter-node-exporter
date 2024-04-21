#![no_std]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Ipv4Event {
    pub src_addr: u32,
    pub dst_addr: u32,
    pub src_port: u16,
    pub dst_port: u16,
}

impl From<&[u8]> for Ipv4Event {
    fn from(v: &[u8]) -> Self {
        let a =
            ((v[0] as u32) << 24) + ((v[1] as u32) << 16) + ((v[2] as u32) << 8) + (v[3] as u32); // This field is expected as host byte order
        let b =
            ((v[7] as u32) << 24) + ((v[6] as u32) << 16) + ((v[5] as u32) << 8) + (v[4] as u32); // network byte order
        let c = ((v[8] as u16) << 8) + (v[9] as u16);
        let d = ((v[10] as u16) << 8) + (v[11] as u16);
        Self {
            src_addr: a,
            dst_addr: b,
            src_port: c,
            dst_port: d,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Ipv6Event {
    pub src_addr: u128,
    pub dst_addr: u128,
    pub src_port: u16,
    pub dst_port: u16,
}
