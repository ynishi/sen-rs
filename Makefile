.PHONY: help preflight publish test check build clean doc release-check release release-patch release-minor
.PHONY: example-check example-build example-run example-clean

help:
	@echo "Available targets:"
	@echo "  make check          - Run cargo check on all crates"
	@echo "  make test           - Run all tests"
	@echo "  make build          - Build all crates"
	@echo "  make doc            - Generate documentation"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make preflight      - Run all checks before commit/publishing"
	@echo ""
	@echo "Example targets:"
	@echo "  make example-check  - Check example CLI"
	@echo "  make example-build  - Build example CLI"
	@echo "  make example-run    - Run example CLI (admin status)"
	@echo "  make example-clean  - Clean example CLI"
	@echo ""
	@echo "Release targets:"
	@echo "  make release-check  - Dry-run release with cargo-release"
	@echo "  make release        - Release patch version (0.x.y -> 0.x.y+1)"
	@echo "  make release-patch  - Release patch version (same as release)"
	@echo "  make release-minor  - Release minor version (0.x.y -> 0.x+1.0)"
	@echo "  make publish        - Publish to crates.io (macros first, then core)"

check:
	@echo "🔍 Checking all crates..."
	cargo check --workspace --all-targets

test:
	@echo "🧪 Running tests..."
	cargo test --workspace --all-targets
	cargo test --workspace --doc

build:
	@echo "🔨 Building all crates..."
	cargo build --workspace

doc:
	@echo "📚 Generating documentation..."
	cargo doc --workspace --no-deps --open

clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean

# Example targets
example-check:
	@echo "🔍 Checking example CLI..."
	cd examples/simple-cli && cargo check

example-build:
	@echo "🔨 Building example CLI..."
	cd examples/simple-cli && cargo build
	@echo "✅ Binary: examples/simple-cli/target/debug/admin"

example-run: example-build
	@echo "🚀 Running example CLI..."
	cd examples/simple-cli && ./target/debug/admin status

example-clean:
	@echo "🧹 Cleaning example CLI..."
	cd examples/simple-cli && cargo clean

preflight:
	@echo "🚦 Running preflight checks..."
	@echo ""
	@echo "1️⃣  Formatting code..."
	cargo fmt --all
	@echo ""
	@echo "2️⃣  Running clippy (auto-fix)..."
	cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged -- -D warnings
	@echo ""
	@echo "3️⃣  Running tests..."
	cargo test --workspace --all-targets
	cargo test --workspace --doc
	@echo ""
	@echo "4️⃣  Checking example..."
	@$(MAKE) example-check
	@echo ""
	@echo "✅ All preflight checks passed!"

release-check:
	@echo "🔍 Dry-run release with cargo-release..."
	@echo ""
	@echo "Note: Install cargo-release if not already installed:"
	@echo "  cargo install cargo-release"
	@echo ""
	@echo "Checking patch release (0.x.y -> 0.x.y+1)..."
	cargo release patch

release-patch: preflight
	@echo "🚀 Releasing PATCH version with cargo-release..."
	@echo ""
	@echo "This will:"
	@echo "  - Update version numbers (0.x.y -> 0.x.y+1)"
	@echo "  - Create git commit and tag"
	@echo "  - (Publish step is manual, see make publish)"
	@echo ""
	@read -p "Continue? [y/N] " confirm && [ "$$confirm" = "y" ] || exit 1
	cargo release patch --execute --no-confirm

release-minor: preflight
	@echo "🚀 Releasing MINOR version with cargo-release..."
	@echo ""
	@echo "This will:"
	@echo "  - Update version numbers (0.x.y -> 0.x+1.0)"
	@echo "  - Create git commit and tag"
	@echo "  - (Publish step is manual, see make publish)"
	@echo ""
	@read -p "Continue? [y/N] " confirm && [ "$$confirm" = "y" ] || exit 1
	cargo release minor --execute --no-confirm

release: release-patch

publish: preflight
	@echo ""
	@echo "🚀 Starting sequential publish process..."
	@echo ""

	@echo "--- Step 1: Publishing sen-rs-macros ---"
	@echo "  Running dry-run for sen-rs-macros..."
	cargo publish -p sen-rs-macros --dry-run --allow-dirty

	@echo "  ✓ Dry-run successful for sen-rs-macros"
	@echo "  Publishing sen-rs-macros to crates.io..."
	cargo publish -p sen-rs-macros --allow-dirty

	@echo ""
	@echo "✅ sen-rs-macros published successfully!"
	@echo ""
	@echo "⏳ Waiting 30 seconds for crates.io index to update..."
	sleep 30

	@echo ""
	@echo "--- Step 2: Publishing sen ---"
	@echo "  Running dry-run for sen..."
	cargo publish -p sen --dry-run --allow-dirty

	@echo "  ✓ Dry-run successful for sen"
	@echo "  Publishing sen to crates.io..."
	cargo publish -p sen --allow-dirty

	@echo ""
	@echo "✅ sen published successfully!"
	@echo ""
	@echo "🎉 All crates have been successfully published to crates.io!"
