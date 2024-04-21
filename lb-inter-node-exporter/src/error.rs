use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("regex error: {0}")]
    Regex(#[source] regex::Error),

    #[error("std::io error: {0}")]
    StdIo(#[source] std::io::Error),

    #[error("netlink error: {0}")]
    Netlink(#[source] rtnetlink::Error),

    #[error("kubernetes error: {0}")]
    Kube(#[source] kube::Error),

    #[error("kubernetes watcher error: {0}")]
    KubeWatcher(#[source] kube::runtime::watcher::Error),

    #[error("Failed to get eBPF Map: {0}")]
    FailedGetEBPFMap(String),
}
