# Local build entry points. CI has its own job; these are for a checkout on a Mac.

APP := dist/DrawKit.app

.PHONY: app run test clean-app ios run-ios format

app:
	scripts/bundle-macos.sh

run: app
	open $(APP)

test:
	cargo test
	swift build --package-path app-macos

format:
	scripts/format.sh

ios:
	scripts/build-ios.sh simulator

run-ios: ios
	xcrun simctl install booted build/ios/DrawKit.app
	xcrun simctl launch booted dev.drawkit.ipados

clean-app:
	rm -rf dist build
