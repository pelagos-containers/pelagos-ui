# pelagos-ui Manual Testing Plan

Repeatable spot-check for all user-facing features.  Run after every significant
change or release.  Each section is independent — you can run them in any order.

---

## Prerequisites

Before starting, confirm the stack is fully up:

```bash
pelagos vm status          # expect: running (pid NNNNN)
pelagos ps                 # expect: no error (empty or container list)
pelagos image ls           # expect: image list, no error
```

Launch the app:

```bash
cd ~/Projects/pelagos-ui
npm run tauri dev
```

---

## 1. Tray Icon — VM Status Indicator

| # | Action | Expected |
|---|--------|----------|
| 1.1 | VM is running; observe tray icon | Filled circle, visible in both light and dark menubar |
| 1.2 | Switch macOS between Light and Dark mode (System Preferences) | Icon remains visible/contrasting in both modes (template image auto-inverts) |
| 1.3 | Open tray menu while VM is running | "Stop VM" is enabled; "Start VM" is greyed out |
| 1.4 | Click "Stop VM" from tray menu | Within ~5 s the icon changes to ring (stopped); "Start VM" becomes enabled, "Stop VM" greys out |
| 1.5 | Click "Start VM" from tray menu | Within ~10 s the icon changes back to filled circle; menu items flip back |
| 1.6 | Click the tray icon directly (left-click) | Dashboard window opens (or comes to front) |
| 1.7 | Click "Open Dashboard" from tray menu | Dashboard window opens (or comes to front) |

---

## 2. Dashboard — Container List

### 2a. Normal operation

| # | Action | Expected |
|---|--------|----------|
| 2.1 | Open dashboard with VM running and at least one container | Container table appears with Name, Status, Image, Command, Age, PID columns |
| 2.2 | Observe a running container row | Status badge shows "running" |
| 2.3 | Wait 5 s without interaction | Container list auto-refreshes (Age column increments) |
| 2.4 | Stop a running container with `pelagos stop <name>` in terminal | Dashboard reflects "exited" status within ~5 s |

### 2b. Stop and remove actions

| # | Action | Expected |
|---|--------|----------|
| 2.5 | Click "Stop" on a running container row | Container transitions to "exited" in the list |
| 2.6 | Click "Remove" on an exited container row | Row disappears from the list |
| 2.7 | Click "Stop" on an already-exited container | Error message appears (red text below header) — pelagos will report it |

### 2c. VM stopped state

| # | Action | Expected |
|---|--------|----------|
| 2.8 | Stop the VM: `pelagos vm stop` | Dashboard shows grey "VM stopped" message (not red error) within ~5 s |
| 2.9 | Restart the VM: `pelagos vm start`; wait for it to come up | "VM stopped" message clears; container list repopulates |

---

## 3. Run Panel — Launch Containers

Open the Run panel: click **+ Run** in the header.

### 3a. Background mode

| # | Action | Expected |
|---|--------|----------|
| 3.1 | Type `alpine:latest` in Image field; leave Name and Command empty; ensure "Background" is selected; click "Run" | Panel closes; within ~5 s a new container row appears in the list |
| 3.2 | Open Run panel; fill Image `alpine:latest`, Name `my-sleep`, Command `sleep 60`; click "Run" | Panel closes; container named "my-sleep" appears in list |
| 3.3 | Open Run panel; leave Image empty; observe "Run" button | Button is disabled (cannot click) |
| 3.4 | Open Run panel; click ✕ | Panel closes without running anything |
| 3.5 | Press Enter in the Image field | Same as clicking "Run" |

### 3b. Interactive mode

| # | Action | Expected |
|---|--------|----------|
| 3.6 | Fill Image `alpine:latest`, Name `itest`, Command `/bin/sh`; select "Interactive"; click "Open terminal" | A new Terminal.app window opens running `pelagos run --tty --interactive --name itest alpine:latest /bin/sh`; panel closes |
| 3.7 | In the opened terminal, type `hostname` | Prints a container hostname (not your Mac hostname) |
| 3.8 | Type `exit` in the terminal | Terminal window closes (or shows exit); container appears as "exited" in dashboard |

### 3c. Mutual exclusion

| # | Action | Expected |
|---|--------|----------|
| 3.9 | Open Run panel, then click "Images" button | Run panel closes; Images view opens |

---

## 4. Images View — Image Management

Open the Images view: click **Images** in the header.

### 4a. List

| # | Action | Expected |
|---|--------|----------|
| 4.1 | Open Images view with cached images | Table lists images with Reference, Digest (12 hex chars), and Layers count |
| 4.2 | Click ↺ (refresh) | List reloads (brief "Loading…" flash) |
| 4.3 | Open Images view with VM stopped | Error message appears describing the connection failure |

### 4b. Pull

| # | Action | Expected |
|---|--------|----------|
| 4.4 | Click "+ Pull"; type `busybox:latest` in the input; click "Pull" | Button changes to "Pulling…"; streaming log output appears as the pull progresses |
| 4.5 | Pull completes successfully | Pull dialog closes; image list refreshes and `busybox:latest` appears |
| 4.6 | Pull a non-existent image: `notarealimage:xyz` | Error message appears in pull dialog (exit code != 0 or stderr) |
| 4.7 | Click "+ Pull"; fill a reference; click ✕ without pulling | Pull dialog closes cleanly; no pull started |
| 4.8 | Press Enter in the pull input | Same as clicking "Pull" |

### 4c. Remove

| # | Action | Expected |
|---|--------|----------|
| 4.9 | Click "Remove" on an image | Confirmation overlay appears with the full image reference and Remove/Cancel buttons |
| 4.10 | Click "Cancel" in the confirm dialog | Dialog closes; image list unchanged |
| 4.11 | Click "Remove" to confirm | Image removed; list refreshes; image no longer appears |
| 4.12 | Try to remove an image that is in use by a running container | Error message appears inside the confirm dialog (pelagos refuses) |

---

## 5. Window Behaviour

| # | Action | Expected |
|---|--------|----------|
| 5.1 | Click the red close button (×) on the dashboard window | Window hides; app remains in tray (does not quit) |
| 5.2 | Re-open via tray icon click | Window reappears at same position |
| 5.3 | Click "Quit" from tray menu | App exits completely; icon disappears from menubar |

---

## 6. Regression Checks

Quick sanity checks that previously-fixed bugs have not regressed:

| # | Check | Expected |
|---|-------|----------|
| 6.1 | Open dashboard immediately after `pelagos vm start` (VM still starting) | Shows "loading…" briefly, then populates — does not hang indefinitely |
| 6.2 | Only one tray icon in menubar | Exactly one pelagos icon visible (no duplicate) |
| 6.3 | Stop VM; observe dashboard error | Shows grey "VM stopped" (not red "I/O error: No such file or directory") |
| 6.4 | Open Run panel; switch Background → Interactive → Background | Button label changes: "Run" / "Open terminal" / "Run" |

---

## Teardown

Clean up any test containers and images created during the run:

```bash
pelagos ps --all               # list all containers including exited
pelagos rm <name>              # remove each test container
pelagos image rm busybox:latest  # remove pull-test image if present
pelagos vm stop                  # optional: stop VM when done
```
