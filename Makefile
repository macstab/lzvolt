# lzvolt
#
# `make help` lists everything. `make check` is what CI runs and what a pull
# request has to pass.

SHELL := /bin/bash
.DEFAULT_GOAL := help

CARGO   ?= cargo
DOCKER  ?= docker
DIST    ?= dist
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

# The comparison benchmarks and the interop soak link the system liblz4.
# build.rs finds it; this only asks for the feature.
LZ4_FEATURE := --features liblz4

# Every released shared object: two libcs by two architectures.
VARIANTS := glibc-amd64 glibc-arm64 musl-amd64 musl-arm64

.PHONY: help
help: ## Show this help
	@echo "lzvolt $(VERSION)"
	@echo
	@grep -hE '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
	  | sort \
	  | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "  Release binaries land in $(DIST)/."

# ---- building ---------------------------------------------------------------

.PHONY: build
build: ## Debug build
	$(CARGO) build

.PHONY: release
release: ## Release build: rlib, staticlib and cdylib
	$(CARGO) build --release

.PHONY: no-asm
no-asm: ## Release build with the assembly off, on the portable reference
	$(CARGO) build --release --no-default-features

# ---- testing ----------------------------------------------------------------

.PHONY: test
test: ## The suite, with the assembly and then without it
	$(CARGO) test --release $(LZ4_FEATURE)
	$(CARGO) test --release --no-default-features

.PHONY: test-fast
test-fast: ## The suite once, debug, for a quick loop
	$(CARGO) test

.PHONY: miri
miri: ## Undefined behaviour check, scaled to run per change (needs nightly)
	MIRIFLAGS="-Zmiri-disable-isolation" \
	  $(CARGO) +nightly miri test --no-default-features --lib format

.PHONY: miri-full
miri-full: ## The same at full size. Hours, not minutes. Scheduled, not per change.
	MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-ignore-leaks" \
	  LZVOLT_MIRI_FULL=1 \
	  $(CARGO) +nightly miri test --no-default-features --lib format -- --nocapture

# Rust has no UBSan. `-Zsanitizer` takes address, thread, memory, leak and
# friends -- `undefined` is not on the list, and asking for it fails the build
# rather than silently doing nothing. Undefined behaviour is Miri's job here,
# plus `-Zub-checks`, which turns on the standard library's own UB assertions.
# The two together are what this target means by "sanitised".
.PHONY: sanitize
sanitize: asan ub-checks ## AddressSanitizer and the library UB checks (needs nightly)

.PHONY: asan
asan: ## AddressSanitizer. Covers the assembly, which Miri cannot execute.
	RUSTFLAGS="-Zsanitizer=address" \
	  $(CARGO) +nightly test --release --target $(shell rustc -vV | sed -n 's/host: //p') --lib

.PHONY: ub-checks
ub-checks: ## The standard library's UB assertions, at release optimisation
	RUSTFLAGS="-Zub-checks -Cdebug-assertions=on" \
	  $(CARGO) +nightly test --release --target $(shell rustc -vV | sed -n 's/host: //p') --lib

.PHONY: soak
soak: ## Push thousands of liblz4-packed values through the kernel
	$(CARGO) run --release $(LZ4_FEATURE) --example soak

# ---- the C ABI --------------------------------------------------------------

.PHONY: c-example
c-example: release ## Compile and run examples/capi.c against the built library
	@mkdir -p $(DIST)
	$(CC) -O2 -Wall -Wextra -Werror -Iinclude \
	  examples/capi.c target/release/liblzvolt.a \
	  -o $(DIST)/capi $(shell [ "$$(uname)" = Darwin ] || echo -lpthread -ldl -lm)
	@echo
	@$(DIST)/capi

.PHONY: header-check
header-check: ## Check the header parses as C99 and as C++ on its own
	@echo '#include "lzvolt.h"' > /tmp/lzvolt_hdr.c
	@echo 'int main(void){return 0;}' >> /tmp/lzvolt_hdr.c
	$(CC) -std=c99 -Wall -Wextra -Werror -Iinclude -fsyntax-only /tmp/lzvolt_hdr.c
	@cp /tmp/lzvolt_hdr.c /tmp/lzvolt_hdr.cpp
	$(CXX) -std=c++17 -Wall -Wextra -Werror -Iinclude -fsyntax-only /tmp/lzvolt_hdr.cpp
	@echo "  header is clean as C99 and C++17"

# ---- quality ----------------------------------------------------------------

.PHONY: fmt
fmt: ## Format the source
	$(CARGO) fmt

.PHONY: fmt-check
fmt-check: ## Fail if the source is not formatted
	$(CARGO) fmt --check

.PHONY: clippy
clippy: ## Lint, warnings are errors
	$(CARGO) clippy --all-targets --release $(LZ4_FEATURE) -- -D warnings
	$(CARGO) clippy --all-targets --release --no-default-features -- -D warnings

.PHONY: doc
doc: ## Build the API documentation, warnings are errors
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --no-deps

.PHONY: check
check: fmt-check clippy doc test header-check c-example ## Everything CI runs
	@echo
	@echo "  all checks passed"

# ---- measuring --------------------------------------------------------------

.PHONY: quick
quick: ## Thumbs up or down in about seven seconds
	$(CARGO) run --release $(LZ4_FEATURE) --example quick

.PHONY: quick-save
quick-save: ## Record the current numbers as the baseline for `make quick`
	$(CARGO) run --release $(LZ4_FEATURE) --example quick -- --save

.PHONY: bench
bench: ## The long form, about twenty minutes, for numbers worth quoting
	$(CARGO) bench $(LZ4_FEATURE)

.PHONY: vectors
vectors: ## Regenerate docs/vectors.txt from the encoder
	$(CARGO) run --release --example make_vectors

# ---- cross-building and release --------------------------------------------

.PHONY: docker-build-all
docker-build-all: $(addprefix docker-build-,$(VARIANTS)) ## Build every released variant
	@echo
	@ls -la $(DIST)/

.PHONY: docker-build-%
docker-build-%: ## Build one variant, e.g. docker-build-musl-arm64
	@set -e; \
	variant='$*'; \
	libc=$${variant%%-*}; arch=$${variant##*-}; \
	case "$$arch" in \
	  amd64) triple=x86_64; platform=linux/amd64 ;; \
	  arm64) triple=aarch64; platform=linux/arm64 ;; \
	  *) echo "unknown architecture: $$arch" >&2; exit 1 ;; \
	esac; \
	case "$$libc" in \
	  glibc) target=$$triple-unknown-linux-gnu;  file=docker/Dockerfile.glibc ;; \
	  musl)  target=$$triple-unknown-linux-musl; file=docker/Dockerfile.musl ;; \
	  *) echo "unknown libc: $$libc" >&2; exit 1 ;; \
	esac; \
	mkdir -p $(DIST); \
	echo "==> $$libc-$$arch ($$target)"; \
	out=$(DIST)/.export-$$libc-$$arch; rm -rf $$out; \
	$(DOCKER) build --platform $$platform -f $$file \
	  --build-arg TARGET=$$target --output type=local,dest=$$out .; \
	mv $$out/liblzvolt.a $(DIST)/liblzvolt-$$libc-$$arch.a; \
	if [ -f $$out/liblzvolt.so ]; then \
	  mv $$out/liblzvolt.so $(DIST)/liblzvolt-$$libc-$$arch.so; \
	else \
	  echo "    (no shared object for this target; the archive is the artifact)"; \
	fi; \
	rm -rf $$out

.PHONY: checksums
checksums: ## SHA256SUMS over everything in dist/
	@cd $(DIST) && { command -v sha256sum > /dev/null \
	  && sha256sum liblzvolt-* lzvolt.h \
	  || shasum -a 256 liblzvolt-* lzvolt.h; } > SHA256SUMS && cat SHA256SUMS

.PHONY: dist
dist: docker-build-all ## Build every variant, add the header, and checksum it
	@cp include/lzvolt.h $(DIST)/
	@$(MAKE) --no-print-directory checksums

.PHONY: changelog
changelog: ## Regenerate CHANGELOG.md from the git history
	git cliff --output CHANGELOG.md

.PHONY: publish-dry
publish-dry: ## What `cargo publish` would send to crates.io
	$(CARGO) publish --dry-run --allow-dirty

# ---- housekeeping -----------------------------------------------------------

.PHONY: clean
clean: ## Remove build output
	$(CARGO) clean
	rm -rf $(DIST)

.PHONY: version
version: ## Print the version in Cargo.toml
	@echo $(VERSION)
