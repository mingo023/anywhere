POCKETD := packages/pocketd/bin/pocketd
SERVICE := gui/$(shell id -u)/dev.mingo.anywhere.dev.pocketd

.PHONY: all pocketd desktop run app

all: desktop

pocketd:
	cd packages/pocketd && go build -o bin/pocketd ./cmd/pocketd
	launchctl print $(SERVICE) >/dev/null 2>&1 || $(POCKETD) daemon install

desktop: pocketd
	cd packages/desktop && cargo build --release -p pocket

run: desktop
	packages/desktop/target/release/pocket-desktop

app: pocketd
	scripts/bundle-dev.sh
