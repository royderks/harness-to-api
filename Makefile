CARGO ?= cargo
VERSION ?= v0.1.0

.PHONY: build test check demo release
build:
	$(CARGO) build --locked --bin h2a --example harness-fixture
	mkdir -p bin
	cp target/debug/h2a bin/h2a

test: build
	H2A_HARNESS_FIXTURE="$(CURDIR)/target/debug/examples/harness-fixture" $(CARGO) test --locked --all-targets

check: test
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --locked --all-targets -- -D warnings
	./bin/h2a factory validate
	H2A_BIN="$(CURDIR)/bin/h2a" python3 -m unittest discover -s scripts/tests -v

# The demo makes a provider call using your existing official CLI login.
demo: build
	H2A_BIN="$(CURDIR)/bin/h2a" ./demo/run.sh

release:
	./scripts/release.sh $(VERSION)
