.PHONY: setup deps sidecars models signing dev build install icon cli check

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

## Creates the local signing identity `install` signs with (asks for your password once).
signing:
	./scripts/setup-signing.sh

## Builds Calls.app and installs it in /Applications. To update: `git pull && make install`.
install:
	./scripts/install.sh

## Redraws the app icon (scripts/make-icon.py) and every size Tauri needs.
icon:
	uv run --no-project --with pillow scripts/make-icon.py apps/desktop/src-tauri/icons/source.png
	tmp=$$(mktemp -d) && cd apps/desktop \
		&& pnpm tauri icon src-tauri/icons/source.png -o $$tmp >/dev/null 2>&1 \
		&& cd $$tmp && cp 32x32.png 128x128.png 128x128@2x.png icon.icns icon.ico icon.png \
			$(CURDIR)/apps/desktop/src-tauri/icons/ \
		&& rm -rf $$tmp

cli:
	cargo build --release -p calls-cli

check:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
	cd apps/desktop && pnpm exec tsc
