FROM rust:1.76.0 as builder

RUN apt update && \
	cargo install bpf-linker && \
	cargo install cargo-generate

WORKDIR /work
COPY . /work

RUN cargo xtask build-ebpf --release && \
	cargo build --release

# CMD ["tail", "-f", "/dev/null"]

FROM debian:stable

RUN apt update && \
	apt install -y iproute2

COPY --from=builder /work/target/release/lb-inter-node-exporter /usr/local/bin
