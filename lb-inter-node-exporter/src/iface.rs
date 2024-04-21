use crate::error::Error;
use futures::TryStreamExt;
use netlink_packet_route::link::LinkAttribute;
// use netlink_packet_route::link::LinkAttribute;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Iface {
    pub name: String,
    pub index: u32,
}

pub async fn get_ifaces(ifaces: &[String]) -> Result<Vec<Iface>, Error> {
    let mut regex_ifaces = Vec::new();
    for iface in ifaces.iter() {
        let r_iface = Regex::new(&iface).map_err(Error::Regex)?;
        regex_ifaces.push(r_iface);
    }
    let iface_list = list_link().await?;
    tracing::info!(link_list=?iface_list, "Link list");

    let filter = |name: &str| -> bool {
        for r_i in regex_ifaces.iter() {
            if r_i.is_match(name) {
                return true;
            }
        }
        false
    };
    let matched: Vec<Iface> = iface_list.into_iter().filter(|i| filter(&i.name)).collect();

    Ok(matched)
}

pub async fn list_link() -> Result<Vec<Iface>, Error> {
    let (conn, handle, _) = rtnetlink::new_connection().map_err(Error::StdIo)?;
    tokio::spawn(conn);

    let mut ifaces = Vec::new();

    let mut links = handle.link().get().execute();
    while let Some(l) = links.try_next().await.map_err(Error::Netlink)? {
        for attr in l.attributes.into_iter() {
            if let LinkAttribute::IfName(name) = attr {
                ifaces.push(Iface {
                    name,
                    index: l.header.index,
                })
            }
        }
    }
    Ok(ifaces)
}
