GO ?= go
VERSION ?= dev

.PHONY: build test check demo release
build:
	$(GO) build -trimpath -ldflags '-X main.version=$(VERSION)' -o bin/h2a ./cmd/h2a

test:
	$(GO) test -race ./...

check: test
	$(GO) vet ./...
	@test -z "$$($(GO) fmt ./...)" || (echo 'Formatting changed; review and rerun make check.'; exit 1)

# The demo makes a real provider call using your existing official CLI login.
demo: build
	H2A_BIN="$(CURDIR)/bin/h2a" ./demo/run.sh

release:
	./scripts/release.sh $(VERSION)
