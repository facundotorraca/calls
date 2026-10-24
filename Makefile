.PHONY: setup deps sidecars models dev build cli check

## First time setup: everything needed to run the app.
setup: deps sidecars models

deps:
	git submodule update --init
	cd apps/desktop && pnpm install

## Builds whisper-cli from vendor/whisper.cpp (needs cmake).
sidecars:
	./scripts/build-sidecars.sh

## Downloads the Whisper and VAD models (~1.6 GB).
models:
	./scripts/download-models.sh

dev:
	cd apps/desktop && pnpm tauri dev

## Builds Calls.app and a .dmg into target/release/bundle.
build:
	cd apps/desktop && pnpm tauri build

cli:
	cargo build --release -p calls-cli

check:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
	cd apps/desktop && pnpm exec tsc
