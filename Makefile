# knobctl — build, device actions, and the protocol-capture rig.
#
# The rustup toolchain isn't always on PATH, so pin the stable toolchain's bin
# (harmless no-op if it doesn't exist and `cargo` is already on PATH).
export PATH := $(HOME)/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$(PATH)

CARGO ?= cargo
BIN   := ./target/release/knobctl
# Path to the vendor config app's executable, for `make capture` only.
# Not distributed here — override with your own copy:
#   make capture APP=/path/to/Vendor.app/Contents/MacOS/Vendor
APP   ?=
HOOK  := $(CURDIR)/hidhook.dylib
LOG   := /tmp/hidhook.log

# Packaging: version comes from Cargo.toml, target defaults to the host triple.
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
TARGET_TRIPLE ?= $(shell rustc -vV | sed -n 's/^host: //p')
DIST := dist/knobctl-$(VERSION)-$(TARGET_TRIPLE)

# Overridable args: `make rgb MODE=3`, `make set TARGET=cw MACRO=volup`, `make read-all`.
MODE   ?= 0
TARGET ?= press
MACRO  ?= enter

.DEFAULT_GOAL := build
.PHONY: build dist hook arm capture log read read-all watch approve volume rgb latency set clean help

## build the release binary
build:
	$(CARGO) build --release

## package a release archive for this host (same layout CI publishes)
dist: build
	@rm -rf $(DIST) $(DIST).tar.gz
	@mkdir -p $(DIST)
	@cp $(BIN) README.md LICENSE mapping.example.yaml $(DIST)/
	@tar -czf $(DIST).tar.gz -C dist $(notdir $(DIST))
	@cd dist && shasum -a 256 $(notdir $(DIST)).tar.gz > $(notdir $(DIST)).tar.gz.sha256
	@echo "packaged $(DIST).tar.gz"

## build the universal (x86_64+arm64) hid_write capture hook
hook: hidhook.dylib
hidhook.dylib: hidhook.c
	clang -dynamiclib -arch x86_64 -arch arm64 -o $@ $< -undefined dynamic_lookup

## clear the capture log
arm:
	@rm -f $(LOG); echo "log armed: $(LOG)"

## launch the vendor app through the hook to capture its HID traffic (set APP=...; quit the app to stop)
capture: hook arm
	@test -n "$(APP)" || { echo "set APP=/path/to/Vendor.app/Contents/MacOS/Vendor"; exit 1; }
	DYLD_INSERT_LIBRARIES=$(HOOK) $(APP)

## print the capture log
log:
	@cat $(LOG) 2>/dev/null || echo "no log yet — run 'make capture'"

## read the bindings stored on the device (knob actions + combos)
read: build
	$(BIN) read

## read every slot, including phantom keyboard defaults
read-all: build
	$(BIN) read --all

## watch live knob output (needs Input Monitoring; runs with sudo)
watch: build
	sudo $(BIN) watch

## bind knob press -> Enter (approves Claude Code prompts)
approve: build
	$(BIN) set press enter

## volume knob: cw = volume up, ccw = volume down
volume: build
	$(BIN) set cw volup
	$(BIN) set ccw voldown

## set an RGB effect mode: make rgb MODE=3
rgb: build
	$(BIN) rgb $(MODE)

## set mouse report latency in ms: make latency MS=100
MS ?= 100
latency: build
	$(BIN) latency $(MS)

## program any binding: make set TARGET=cw MACRO=volup
set: build
	$(BIN) set $(TARGET) $(MACRO)

## remove cargo build artifacts and packaged archives (keeps the capture hook)
clean:
	$(CARGO) clean
	rm -rf dist

## list targets
help:
	@awk '/^## /{sub(/^## /,"");c=$$0;next} \
	      /^[a-zA-Z][a-zA-Z0-9_-]*:/{split($$0,a,":");if(c!=""){printf "  \033[36m%-10s\033[0m %s\n",a[1],c;c=""}}' \
	      $(MAKEFILE_LIST)
