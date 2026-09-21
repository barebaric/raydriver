.PHONY: build dev stubs format format-rust format-python lint lint-rust lint-python test check venv

build:
	maturin build --release --out dist

dev: install-test-deps
	maturin develop --profile dev

install-test-deps:
	pip install -e ".[test]" --quiet

venv:
	python3 -m venv .venv
	@echo "Run 'source .venv/bin/activate' to activate the virtual environment."

stubs:
	cargo clean -p raydriver && cargo run --bin stub_gen

format: format-rust format-python

format-rust:
	cargo fmt

format-python:
	ruff format tests/ python/
	ruff check --fix tests/ python/

lint: lint-rust lint-python

lint-rust:
	cargo fmt --check
	cargo clippy -- -D warnings

lint-python:
	ruff check tests/ python/
	ruff format --check tests/ python/
	npx pyright python/raydriver tests

test:
	pytest -v

check: lint test
