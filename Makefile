.PHONY: build dev stubs format format-rust format-python lint lint-rust lint-python test check check-wasm check-targets doc docs install-docs-deps venv

build:
	maturin build --release --out dist

dev: install-test-deps
	maturin develop --profile dev

install-test-deps:
	pip install -e ".[test]" --quiet

install-docs-deps:
	pip install -e ".[docs]" --quiet

venv:
	python3 -m venv .venv
	@echo "Run 'source .venv/bin/activate' to activate the virtual environment."

stubs:
	cargo clean -p raydriver && cargo run --features python --bin stub_gen

format: format-rust format-python

format-rust:
	cargo fmt

format-python:
	ruff format tests/ python/ tools/
	ruff check --fix tests/ python/ tools/

lint: lint-rust lint-python

lint-rust:
	cargo fmt --check
	cargo clippy --features python -- -D warnings
	cargo clippy --no-default-features -- -D warnings
	# The opt-in TLS root provider must stay buildable (native roots
	# are covered by the `python` build above).
	cargo check --no-default-features --features tls-webpki-roots

lint-python:
	ruff check tests/ python/ tools/
	ruff format --check tests/ python/ tools/
	npx pyright python/raydriver tests tools

test:
	pytest -v

check: lint test

# The wasm target builds with no features at all — and keeps
# everything a browser can drive: session logic, HTTP/WS clients,
# telnet transport, esp3d/octoprint/xtool sessions (dialing via the
# connector seam, see docs/wasm.md).  Requires `rustup target add
# wasm32-unknown-unknown`.
check-wasm:
	cargo check --no-default-features
	cargo check --target wasm32-unknown-unknown --no-default-features

# Cross-check the platform-gated code (bluetooth backends etc.) that
# a linux build never compiles.  Requires the rustup targets; ring
# is not part of these checks, so no C cross-toolchain is needed.
check-targets:
	cargo check --target x86_64-pc-windows-gnu --no-default-features
	cargo check --target aarch64-apple-darwin --no-default-features

doc docs: install-docs-deps
	python -m tools.cli all
