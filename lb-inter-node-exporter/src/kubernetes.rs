use std::{net::IpAddr, pin::pin, str::FromStr};

use futures::TryStreamExt;
use k8s_openapi::api::core::v1::Service;
use kube::{
    runtime::{watcher, WatchStreamExt},
    Api, Client, ResourceExt,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VipEvent {
    Add(Lb),
    Delete(Lb),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lb {
    pub name: String,
    pub namespace: String,
    pub addr: Option<IpAddr>,
}

pub struct ServiceWatcher {
    client: Client,
    // ebpf map
    vip_events: UnboundedSender<VipEvent>,
}

impl ServiceWatcher {
    pub async fn new(vip_events: UnboundedSender<VipEvent>) -> Self {
        let client = Client::try_default()
            .await
            .expect("Failed to create kube client");
        ServiceWatcher { client, vip_events }
    }

    #[tracing::instrument(skip_all)]
    pub async fn run(&self) -> Result<(), Error> {
        let svc_api = Api::<Service>::all(self.client.clone());
        let watcher_config = watcher::Config::default();
        let svc_events = watcher(svc_api, watcher_config)
            .default_backoff()
            .touched_objects();
        let mut svc_events = pin!(svc_events);

        tracing::info!("Start Service watcher");
        while let Some(svc) = svc_events.try_next().await.map_err(Error::KubeWatcher)? {
            let ns = svc.namespace().unwrap();
            let namespaced_svc_api = Api::<Service>::namespaced(self.client.clone(), &ns);
            match namespaced_svc_api.get_opt(&svc.name_any()).await {
                Ok(svc_opt) => match svc_opt {
                    Some(svc) => {
                        if let Some(vip) = get_lb_addr(&svc) {
                            if let Ok(vip) = IpAddr::from_str(&vip) {
                                self.vip_events
                                    .send(VipEvent::Add(Lb {
                                        name: svc.name_any(),
                                        namespace: ns.clone(),
                                        addr: Some(vip),
                                    }))
                                    .unwrap();
                            }
                        }
                    }
                    None => {
                        self.vip_events
                            .send(VipEvent::Delete(Lb {
                                name: svc.name_any(),
                                namespace: ns.clone(),
                                addr: None,
                            }))
                            .unwrap();
                    }
                },
                Err(e) => {
                    tracing::warn!(name=svc.name_any(), namespace = ns, error=?e, "Failed to get svc");
                }
            }
        }

        Ok(())
    }
}

fn get_lb_addr(svc: &Service) -> Option<String> {
    if let Some(svc_spec) = svc.spec.as_ref() {
        if let Some(svc_type) = svc_spec.type_.as_ref() {
            if svc_type.ne("LoadBalancer") {
                return None;
            }
        } else {
            return None;
        }
        if let Some(etp) = svc_spec.external_traffic_policy.as_ref() {
            if etp.eq("Local") {
                return None;
            }
        }
    } else {
        return None;
    }

    if let Some(svc_status) = svc.status.as_ref() {
        if let Some(lb_status) = svc_status.load_balancer.as_ref() {
            if let Some(ingress) = lb_status.ingress.as_ref() {
                if let Some(lb_ingress) = ingress.first() {
                    return lb_ingress.ip.clone();
                }
            }
        }
    }
    None
}
