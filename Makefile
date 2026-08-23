.PHONY: lint test ci fmt lint-rust lint-py test-rust test-py

lint-py:
	@echo "Running Python linting..."
	@uvx ruff check python-bindings
	@uv --directory python-bindings run pyrefly check

lint-rust:
	@echo "Running Rust linting"
	@cargo fmt --check
	@cargo clippy --all-targets --all-features -- -D warnings

lint: lint-rust lint-py

test-rust:
	@echo "Running Rust tests"
	@cargo test -p core

test-py:
	@echo "Running Python tests"
	@uv --directory python-bindings run pytest

test: test-rust test-py

fmt:
	@echo "Running formatters"
	@cargo fmt
	@uvx ruff check --fix python-bindings
	@uvx ruff format python-bindings

ci: lint test
