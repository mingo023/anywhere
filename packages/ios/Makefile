KIT := Packages/AnywhereKit
SWIFT := PATH="$(HOME)/.swiftly/bin:$$PATH" swift

.PHONY: gen build test bench

# The Xcode project for the app and its extensions; building it needs Xcode.
gen:
	@command -v xcodegen >/dev/null || { echo "xcodegen isn't installed: brew install xcodegen"; exit 1; }
	xcodegen generate

build:
	cd $(KIT) && $(SWIFT) build

test:
	cd $(KIT) && $(SWIFT) test

bench:
	cd $(KIT) && $(SWIFT) run -c release anywhere-bench
