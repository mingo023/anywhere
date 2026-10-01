POCKETD := packages/pocketd/bin/pocketd
SERVICE := gui/$(shell id -u)/dev.mingo.anywhere.pocketd

.PHONY: all pocketd desktop run app

all: desktop

pocketd:
	cd packages/pocketd && go build -o bin/pocketd ./cmd/pocketd
	if launchctl print $(SERVICE) >/dev/null 2>&1; then launchctl kickstart -k $(SERVICE); else $(POCKETD) daemon install; fi

desktop: pocketd
	cd packages/desktop && cargo build --release -p pocket

run: desktop
	packages/desktop/target/release/pocket-desktop

app: pocketd
	scripts/bundle-dev.sh
