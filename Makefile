debug ?=

ifdef debug
  release :=
  target :=debug
  extension :=debug
else
  release :=--release
  target :=release
  extension :=
endif

build:
	cargo build $(release)

test:
	cargo test 

registry:
	cargo run --bin registry

rust-parser:
	cargo run --bin rust_parser

discoverer:
	cargo run --bin discoverer

