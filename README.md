# lb-inter-node-exporter

`lb-inter-node-exporter` is the exporter to trace the intermediate node of Kubernetes LoadBalancer service with `externalTrafficPolicy=Cluster`.

This only supports Ipv4 now.

This project is `Rust` and [aya-rs](https://github.com/aya-rs/aya).

## Build

To build `lb-inter-node-exporter`, we need Rust and its toolchain.
Please run following commands.

```console
$ make setup
$ make build
```

## Quick start

We can try with kind.

1. Run following commands

```console
$ make start # start kind and containerlab
$ make metallb # deploy metallb
$ make apply # deploy lb-inter-node-exporter
$ make lb # deploy sample workloads
```

After preparing the environment, we may have to wait a few minute establishing the connectivity via LoadBalancer.

2. Check the connectivity with sample workloads via LoadBalancer

```console
$ kubectl -n test get svc
NAME               TYPE           CLUSTER-IP       EXTERNAL-IP   PORT(S)        AGE
app-svc-cluster    LoadBalancer   10.101.85.137    10.0.10.0     80:32554/TCP   3s
app-svc-cluster2   LoadBalancer   10.101.180.159   10.0.10.1     80:31924/TCP   3s
app-svc-local      LoadBalancer   10.101.241.182   10.0.10.2     80:31789/TCP   3s
$ docker exec -it  clab-lb-inter-node-exporter-client0 curl http://10.0.10.0
{"timestamp":"2024-04-21T07:35:35.221203969Z","from":"10.100.1.2","to":"172.18.0.4:21172"}%
```

3. Check the log and metrics

```console
stern -n kube-system ds/lb-inter-node-exporter
+ lb-inter-node-exporter-89s6p › lb-inter-node-exporter
+ lb-inter-node-exporter-hssv6 › lb-inter-node-exporter
+ lb-inter-node-exporter-htszd › lb-inter-node-exporter
+ lb-inter-node-exporter-ncv7t › lb-inter-node-exporter
...(snip)...
lb-inter-node-exporter-89s6p lb-inter-node-exporter {"timestamp":"2024-04-21T07:35:35.221020Z","level":"INFO","fields":{"message":"Received by intermediate node","src_addr":"2.0.168.192","dst_addr":"10.0.10.0","src_port":60618,"dst_port":80},"target":"lb_inter_node_exporter"}
...(snip)...
```

```console
docker exec -it lb-inter-node-exporter-worker2 curl localhost:8080/metrics
# HELP lb_inter_node_exporter_picked_total The count of picked as the intermediate node
# TYPE lb_inter_node_exporter_picked_total counter
lb_inter_node_exporter_picked_total{dst="10.0.10.0",src="192.168.0.2"} 2
```

4. Clean up the test environment

```console
$ make stop
```
