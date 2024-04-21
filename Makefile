
PROJECT := lb-inter-node-exporter

KIND_VERSION := 0.22.0
KUBERNETES_VERSION := 1.29.2
KUSTOMIZE_VERSION := 5.2.1
METALLB_VERSION := 0.14.4
CONTAINERLAB_VERSION := 0.54.2

BINDIR := $(abspath $(PWD)/bin)
KIND := $(BINDIR)/kind
KUBECTL := $(BINDIR)/kubectl
KUSTOMIZE := $(BINDIR)/kustomize
CONTAINERLAB := $(BINDIR)/containerlab

KIND_CONFIG := kind.yaml
LAB_CONFIG := lab.yaml

SUDO ?= sudo

.PHONY: setup
setup: $(KIND) $(KUBECTL) $(KUSTOMIZE) $(CONTAINERLAB)
	cargo install bpf-linker

.PHONY: build
build: setup
	cargo xtask build-ebpf
	cargo build

TAG ?= dev
.PHONY: build-image
build-image:
	docker build -t lb-inter-node-exporter:$(TAG) .

.PHONY: start
start:
	$(KIND) create cluster --image kindest/node:v$(KUBERNETES_VERSION) --name $(PROJECT) --config=$(KIND_CONFIG)
	$(KIND) load docker-image $(PROJECT):dev -n $(PROJECT)
	$(SUDO) $(CONTAINERLAB) deploy -t $(LAB_CONFIG)

.PHONY: stop
stop:
	$(SUDO) $(CONTAINERLAB) destroy -t $(LAB_CONFIG)
	$(KIND) delete cluster --name $(PROJECT)

.PHONY: apply
apply:
	$(KUSTOMIZE) build manifests/base | $(KUBECTL) apply -f -

.PHONY: metallb
metallb:
	$(KUBECTL) apply -f https://raw.githubusercontent.com/metallb/metallb/v$(METALLB_VERSION)/config/manifests/metallb-native.yaml
	$(KUBECTL) -n metallb-system wait --timeout=300s --for=condition=available deploy/controller
	sleep 3
	$(KUSTOMIZE) build manifests/base/metallb | $(KUBECTL) apply -f -

.PHONY: lb
lb:
	$(KUSTOMIZE) build manifests/base/lb | $(KUBECTL) apply -f -

$(KIND):
	mkdir -p $(dir $@)
	curl -sfL -o $@ https://github.com/kubernetes-sigs/kind/releases/download/v$(KIND_VERSION)/kind-linux-amd64
	chmod a+x $@

$(KUBECTL):
	mkdir -p $(dir $@)
	curl -sfL -o $@ https://dl.k8s.io/release/v$(KUBERNETES_VERSION)/bin/linux/amd64/kubectl
	chmod a+x $@

$(KUSTOMIZE):
	mkdir -p $(dir $@)
	curl -sfL https://github.com/kubernetes-sigs/kustomize/releases/download/kustomize%2Fv$(KUSTOMIZE_VERSION)/kustomize_v$(KUSTOMIZE_VERSION)_linux_amd64.tar.gz | tar -xz -C $(BINDIR)
	chmod a+x $@

$(CONTAINERLAB):
	mkdir -p $(dir $@)
	curl -sfL https://github.com/srl-labs/containerlab/releases/download/v$(CONTAINERLAB_VERSION)/containerlab_$(CONTAINERLAB_VERSION)_Linux_amd64.tar.gz | tar -xz -C $(BINDIR)
	chmod a+x $@
