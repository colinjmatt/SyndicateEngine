.PHONY: fmt test build report validate run

fmt:
	cd syndicate_engine && cargo fmt --all

test:
	cd syndicate_engine && cargo test --all-targets

build:
	cd syndicate_engine && cargo build --all-targets

report:
	cd syndicate_engine && cargo run --bin inspect_assets -- ../original_assets ../docs/generated/asset-report.md

validate: fmt test build report

run:
	cd syndicate_engine && cargo run --bin syndicate_engine