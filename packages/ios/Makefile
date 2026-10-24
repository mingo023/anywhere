KIT := Packages/AnywhereKit
SWIFT := PATH="$(HOME)/.swiftly/bin:$$PATH" swift

.PHONY: gen build test bench typecheck-ios

# The Xcode project for the app and its extensions; building it needs Xcode.
gen:
	@command -v xcodegen >/dev/null || { echo "xcodegen isn't installed: brew install xcodegen"; exit 1; }
	xcodegen generate

build:
	cd $(KIT) && $(SWIFT) build

test:
	cd $(KIT) && $(SWIFT) test

# The iOS-only code, typechecked as Mac Catalyst against the Command Line Tools SDK (see scripts/ios-typecheck).
typecheck-ios: build
	PATH="$(HOME)/.swiftly/bin:$$PATH" python3 -I scripts/ios-typecheck/typecheck.py $(KIT)/.build

bench:
	cd $(KIT) && $(SWIFT) run -c release anywhere-bench
