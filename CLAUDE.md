# pelagos-ui — Claude Instructions

## What This Project Is

pelagos-ui is the management GUI for the [pelagos](https://github.com/pelagos-containers/pelagos)
container runtime. It is a hybrid systray + full-window desktop application for macOS and
Linux, built with [Tauri 2](https://tauri.app) (Rust backend) and Svelte 5 + SvelteKit (frontend).

**Relationship to other repos:**
- `pelagos` — Linux container runtime (CLI + library).  The UI calls its CLI (`pelagos ps --format json`, etc.) on Linux.
- `pelagos-mac` — macOS VM layer (AVF + vsock).  On macOS the UI talks to the `pelagos-guest` daemon inside the VM over vsock.
- `pelagos-protocol` (this repo, `crates/pelagos-protocol/`) — the shared Rust types for the vsock IPC channel.

---

## Repository Layout

```
pelagos-ui/
  crates/
    pelagos-protocol/    ← Shared vsock protocol types (Rust, Apache 2.0)
      src/
        lib.rs           ← Re-exports + PROTOCOL_VERSION constant
        command.rs       ← GuestCommand enum (host → guest)
        response.rs      ← GuestResponse enum (guest → host)
        types.rs         ← ContainerInfo, ContainerStatus, VmStatus, etc.
  src-tauri/             ← Tauri Rust backend (to be created)
    src/
      main.rs
      commands.rs        ← #[tauri::command] functions (list_containers, vm_status, …)
      backend/
        mod.rs           ← RuntimeBackend trait
        vsock.rs         ← macOS: JSON-over-vsock client using pelagos-protocol
        process.rs       ← Linux: spawn pelagos CLI subprocesses
  src/                   ← SvelteKit frontend (to be created)
    routes/
      +page.svelte       ← Main dashboard
    lib/
      components/        ← ContainerRow.svelte, VmStatusBadge.svelte, etc.
      stores/            ← containers, vmStatus (Svelte stores)
      ipc.ts             ← Tauri invoke() wrappers
  scripts/
    pelagos-waybar-status ← Shell script for Waybar custom/pelagos module
  package.json
  svelte.config.js
  vite.config.ts
  Cargo.toml             ← Workspace root
  CLAUDE.md              ← This file
```

---

## ⚠️ CRITICAL DESIGN DECISIONS ⚠️

### License Constraint
All dependencies must be Apache 2.0, MIT, MPL 2.0, or 3-clause BSD.  No GPL, no LGPL,
no proprietary licenses.  Check `cargo deny` / `npm audit` before adding any dependency.

### No Subsystem Dependencies
Same principle as `pelagos-mac`: no subsystem-sized external dependencies.  The UI uses
`pelagos` and `pelagos-mac` as subordinate tools, not as subsystems it builds around.

### Platform Abstraction
The `RuntimeBackend` trait (in `src-tauri/src/backend/mod.rs`) is the boundary:
- macOS implementation talks vsock to `pelagos-guest`
- Linux implementation spawns `pelagos` CLI subprocesses

The Svelte frontend and Tauri commands never know which backend is active.

### Protocol Stability
`crates/pelagos-protocol` is the **source of truth** for the vsock wire format.
When you need to add a command:
1. Add the variant to `GuestCommand` in `command.rs`
2. Add handling to `pelagos-guest/src/main.rs` in the pelagos-mac repo
3. Bump `PROTOCOL_VERSION` if the change is not backward-compatible

Never add ad-hoc JSON shapes outside of `pelagos-protocol`.

---

## Milestone 0 Scope (current)

**What ships:**
- Systray icon with VM status indicator (macOS: running/starting/stopped; Linux: N/A)
- Right-click menu: Open Dashboard, Start VM, Stop VM, Quit
- Dashboard window: live container list with name, image, status, uptime, pid
- Per-container actions: Stop, Remove

**Backend status:**
- `src-tauri/src/backend/vsock.rs`: implemented; `list_containers` blocked on structured JSON over vsock (see [pelagos-containers/pelagos-mac](https://github.com/pelagos-containers/pelagos-mac))
- `src-tauri/src/backend/process.rs`: implemented
- `src-tauri/src/commands.rs`: implemented

**Remaining work:**
- Wire up vsock `list_containers` once pelagos-guest exposes `GuestCommand::Ps { json: true }`
- Frontend: connect Svelte stores to Tauri commands
- Systray: VM start/stop actions via vsock

---

## Waybar Integration (Linux)

The `scripts/pelagos-waybar-status` script is a zero-dependency Waybar custom module:
- Calls `pelagos ps --all --format json`
- Outputs `{"text": "N ▶", "tooltip": "...", "class": "running|idle"}` for Waybar
- `on-click` in Waybar config opens `pelagos-ui`

Waybar config snippet:
```json
"custom/pelagos": {
    "exec": "~/.local/bin/pelagos-waybar-status",
    "interval": 5,
    "on-click": "pelagos-ui",
    "format": "󰡨 {}",
    "return-type": "json"
}
```

---

## Development Commands

```bash
# Build the protocol crate only
cargo build -p pelagos-protocol

# Run protocol crate tests
cargo test -p pelagos-protocol

# (Future) Run Tauri dev server
npm run tauri dev

# (Future) Build production app
npm run tauri build
```

---

## Execution Style

Execute quietly — no step-by-step narration. Just do it, then give a short summary of
what was done. Reserve prose for plans, questions, and results.

All tool use is pre-approved: Bash, Read, Edit, Write, Grep, Glob, WebSearch, WebFetch.

### Ask Before Major Decisions
- Protocol changes (new GuestCommand variants, field additions)
- Adding new external dependencies
- Architectural changes to the backend abstraction
- When uncertain about the right approach

---

## No Time Estimates

Never include time estimates in documentation, plans, or commit messages.
Use effort descriptors: "Quick", "Moderate Effort", "Significant Work."
