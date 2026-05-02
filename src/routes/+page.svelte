<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { containers, loading, error, startPolling } from '$lib/stores/containers';
  import ContainerRow from '$lib/components/ContainerRow.svelte';
  import ImagePane from '$lib/components/ImagePane.svelte';
  import LogsPanel from '$lib/components/LogsPanel.svelte';
  import { stopContainer, removeContainer, launchExecWindow } from '$lib/ipc';
  import type { ContainerInfo } from '$lib/ipc';

  let stopPolling: () => void;

  // Logs panel state
  let logsContainer: ContainerInfo | null = null;

  // Exec modal state
  let execContainer: string | null = null;
  let execCmd = 'sh';
  let execInput: HTMLInputElement;

  // Filter + sort state
  let runningOnly = true;

  // Edit / bulk-remove state
  let editMode = false;
  let selected: Set<string> = new Set();
  let prevRunningOnly = true;
  let removing = false;
  type SortCol = 'name' | 'status' | 'image' | 'command' | 'started_at' | 'pid';
  let sortCol: SortCol = 'started_at';
  let sortAsc = false; // default: newest first

  onMount(() => { stopPolling = startPolling(); });
  onDestroy(() => stopPolling?.());

  async function handleStop(name: string) {
    try {
      await stopContainer(name);
    } catch (e) {
      error.set(String(e));
    }
  }

  async function handleRemove(name: string) {
    try {
      await removeContainer(name, false);
    } catch (e) {
      error.set(String(e));
    }
  }

  function handleLogs(name: string) {
    logsContainer = $containers.find(c => c.name === name) ?? null;
  }

  function handleExec(name: string) {
    execContainer = name;
    execCmd = 'sh';
    // Focus the input after the modal renders.
    setTimeout(() => execInput?.focus(), 0);
  }

  async function submitExec() {
    if (!execContainer) return;
    const name = execContainer;
    const cmd = execCmd.trim().split(/\s+/).filter(Boolean);
    execContainer = null;
    try {
      await launchExecWindow(name, cmd.length ? cmd : ['sh']);
    } catch (e) {
      error.set(String(e));
    }
  }

  function cancelExec() {
    execContainer = null;
  }

  function setSort(col: SortCol) {
    if (sortCol === col) {
      sortAsc = !sortAsc;
    } else {
      sortCol = col;
      sortAsc = col !== 'started_at';
    }
  }

  function cmpValue(c: ContainerInfo, col: SortCol): string | number {
    switch (col) {
      case 'name':       return c.name.toLowerCase();
      case 'status':     return c.status === 'running' ? 0 : 1;
      case 'image':      return (c.image ?? c.rootfs).toLowerCase();
      case 'command':    return c.command.join(' ').toLowerCase();
      case 'started_at': return new Date(c.started_at).getTime();
      case 'pid':        return c.pid;
    }
  }

  $: filtered = runningOnly
    ? $containers.filter(c => c.status === 'running')
    : $containers;

  $: sorted = [...filtered].sort((a, b) => {
    const av = cmpValue(a, sortCol);
    const bv = cmpValue(b, sortCol);
    const cmp = av < bv ? -1 : av > bv ? 1 : 0;
    return sortAsc ? cmp : -cmp;
  });

  $: exitedCount = $containers.filter(c => c.status === 'exited').length;

  function indicator(col: SortCol) {
    if (sortCol !== col) return '';
    return sortAsc ? ' ▲' : ' ▼';
  }

  function enterEditMode() {
    prevRunningOnly = runningOnly;
    runningOnly = false;
    selected = new Set($containers.filter(c => c.status === 'exited').map(c => c.name));
    editMode = true;
  }

  function cancelEdit() {
    editMode = false;
    selected = new Set();
    runningOnly = prevRunningOnly;
  }

  function toggleSelected(name: string) {
    const next = new Set(selected);
    if (next.has(name)) next.delete(name); else next.add(name);
    selected = next;
  }

  async function removeSelected() {
    removing = true;
    for (const name of selected) {
      try { await removeContainer(name, false); } catch (e) { error.set(String(e)); }
    }
    removing = false;
    editMode = false;
    selected = new Set();
    runningOnly = prevRunningOnly;
  }
</script>

<div class="app">
  <!-- ── header ──────────────────────────────────────────────────────────── -->
  <header>
    <h1>Pelagos</h1>
    {#if $loading}<span class="hint">loading…</span>{/if}
    {#if $error}<span class={$error === 'VM stopped' ? 'hint' : 'err'}>{$error}</span>{/if}
    {#if !editMode}
      <button
        class="filter-btn"
        class:active={runningOnly}
        on:click={() => (runningOnly = !runningOnly)}
        title={runningOnly ? 'Showing running only — click to show all' : 'Showing all — click to show running only'}
      >{runningOnly ? 'Running' : 'All'}</button>
      {#if exitedCount > 0}
        <button class="edit-btn" on:click={enterEditMode}>Edit</button>
      {/if}
    {:else}
      <button class="remove-btn" on:click={removeSelected} disabled={removing || selected.size === 0}>
        {removing ? 'Removing…' : `Remove selected (${selected.size})`}
      </button>
      <button class="cancel-btn" on:click={cancelEdit} disabled={removing}>Cancel</button>
    {/if}
  </header>

  <!-- ── two-pane layout ─────────────────────────────────────────────────── -->
  <div class="layout">

    <!-- top pane: containers -->
    <div class="pane containers-pane">
      {#if !$loading && sorted.length === 0}
        {#if runningOnly && exitedCount > 0}
          <p class="empty">No running containers — {exitedCount} exited.
            <button class="link-btn" on:click={() => (runningOnly = false)}>Show all</button>
          </p>
        {:else}
          <p class="empty">No containers yet.</p>
        {/if}
      {:else}
        <table>
          <thead>
            <tr>
              <th class="check-th"></th>
              <th><button class="sort-btn" on:click={() => setSort('name')}>Name{indicator('name')}</button></th>
              <th><button class="sort-btn" on:click={() => setSort('status')}>Status{indicator('status')}</button></th>
              <th><button class="sort-btn" on:click={() => setSort('image')}>Image{indicator('image')}</button></th>
              <th><button class="sort-btn" on:click={() => setSort('command')}>Command{indicator('command')}</button></th>
              <th><button class="sort-btn" on:click={() => setSort('started_at')}>Age{indicator('started_at')}</button></th>
              <th><button class="sort-btn" on:click={() => setSort('pid')}>PID{indicator('pid')}</button></th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each sorted as c (c.name)}
              <ContainerRow
                container={c}
                {editMode}
                checked={selected.has(c.name)}
                on:stop={e => handleStop(e.detail)}
                on:remove={e => handleRemove(e.detail)}
                on:toggle={e => toggleSelected(e.detail)}
                on:logs={e => handleLogs(e.detail)}
              on:exec={e => handleExec(e.detail)}
              />
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    <!-- divider -->
    <div class="pane-divider"></div>

    <!-- bottom pane: images -->
    <div class="pane image-pane">
      <div class="pane-header">
        <span class="pane-title">Images</span>
      </div>
      <ImagePane />
    </div>

    <!-- logs pane: shown when a container is selected for logs -->
    {#if logsContainer}
      <div class="pane-divider"></div>
      <div class="pane logs-pane">
        <LogsPanel
          containerName={logsContainer.name}
          isRunning={logsContainer.status === 'running'}
          on:close={() => { logsContainer = null; }}
        />
      </div>
    {/if}

  </div>
</div>

<!-- exec command modal -->
{#if execContainer}
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
  <div class="modal-backdrop" on:click={cancelExec}>
    <div class="modal" on:click|stopPropagation>
      <p class="modal-title">Exec into <strong>{execContainer}</strong></p>
      <input
        bind:this={execInput}
        bind:value={execCmd}
        class="modal-input"
        type="text"
        placeholder="sh"
        on:keydown={e => { if (e.key === 'Enter') submitExec(); else if (e.key === 'Escape') cancelExec(); }}
      />
      <div class="modal-actions">
        <button class="modal-cancel" on:click={cancelExec}>Cancel</button>
        <button class="modal-submit" on:click={submitExec}>Open Terminal</button>
      </div>
    </div>
  </div>
{/if}

<p class="attribution">Photo: Jeri Leandera (CC BY-SA)</p>
<p class="attribution attribution-left">Icon: hibernut / Noun Project (CC BY)</p>

<style>
  :global(*, *::before, *::after) { box-sizing: border-box; }
  :global(body) {
    margin: 0;
    background:
      linear-gradient(rgba(15, 17, 23, 0.80), rgba(15, 17, 23, 0.80)),
      url('/sea-slugs.jpg') center / cover fixed;
    color: #f0f0f0;
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 14px;
  }

  .app    { display: flex; flex-direction: column; height: 100vh; padding: 0; overflow: hidden; }

  header  {
    display: flex;
    align-items: baseline;
    gap: 16px;
    padding: 16px 24px 12px;
    flex-shrink: 0;
    border-bottom: 1px solid #1f2937;
  }
  h1      { margin: 0; font-size: 1.1rem; font-weight: 700; letter-spacing: -0.01em; }
  .hint   { color: #6b7280; font-size: 0.8rem; }
  .err    { color: #f87171; font-size: 0.8rem; }

  .filter-btn {
    margin-left: auto;
    background: transparent;
    border: 1px solid #374151;
    border-radius: 4px;
    color: #9ca3af;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 2px 10px;
  }
  .filter-btn:hover  { border-color: #6b7280; color: #f0f0f0; }
  .filter-btn.active { border-color: #3b82f6; color: #93c5fd; }

  .edit-btn {
    background: transparent;
    border: 1px solid #374151;
    border-radius: 4px;
    color: #9ca3af;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 2px 10px;
  }
  .edit-btn:hover { border-color: #6b7280; color: #f0f0f0; }

  .remove-btn {
    background: #7f1d1d;
    border: 1px solid #991b1b;
    border-radius: 4px;
    color: #fca5a5;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 2px 12px;
  }
  .remove-btn:hover:not(:disabled) { background: #991b1b; }
  .remove-btn:disabled { opacity: 0.5; cursor: default; }

  .cancel-btn {
    background: transparent;
    border: 1px solid #374151;
    border-radius: 4px;
    color: #9ca3af;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 2px 10px;
  }
  .cancel-btn:hover:not(:disabled) { border-color: #6b7280; color: #f0f0f0; }
  .cancel-btn:disabled { opacity: 0.5; cursor: default; }

  .check-th { width: 32px; padding: 0 4px 0 12px; }

  /* two-pane layout */
  .layout {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .pane { overflow-y: auto; }
  .containers-pane {
    flex: 1 1 0;
    min-height: 80px;
    padding: 8px 24px;
  }
  .pane-divider {
    height: 1px;
    background: #30363d;
    flex-shrink: 0;
  }
  .image-pane {
    flex: 1 1 0;
    min-height: 180px;
  }
  .logs-pane {
    flex: 0 0 260px;
    min-height: 120px;
    overflow: hidden;
  }
  .pane-header {
    display: flex;
    align-items: center;
    padding: 10px 24px 0;
  }
  .pane-title {
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #6b7280;
  }

  /* container table */
  .link-btn {
    background: none;
    border: none;
    color: #60a5fa;
    cursor: pointer;
    font-size: inherit;
    padding: 0;
    text-decoration: underline;
  }
  .empty  { color: #6b7280; margin-top: 24px; text-align: center; }
  table   { width: 100%; border-collapse: collapse; }
  th {
    text-align: left;
    padding: 0;
    color: #6b7280;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    border-bottom: 1px solid #1f2937;
  }
  .sort-btn {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font: inherit;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 6px 12px;
    width: 100%;
    text-align: left;
  }
  .sort-btn:hover { color: #f0f0f0; }

  .attribution {
    position: fixed;
    bottom: 8px;
    right: 12px;
    font-size: 0.65rem;
    color: rgba(255, 255, 255, 0.45);
    pointer-events: none;
    user-select: none;
  }
  .attribution-left {
    right: unset;
    left: 12px;
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: #1a1f2e;
    border: 1px solid #374151;
    border-radius: 8px;
    padding: 20px 24px;
    width: 360px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .modal-title {
    margin: 0;
    font-size: 0.85rem;
    color: #d1d5db;
  }
  .modal-title strong { color: #f0f0f0; }
  .modal-input {
    background: #0d1117;
    border: 1px solid #374151;
    border-radius: 4px;
    color: #f0f0f0;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
    padding: 6px 10px;
    width: 100%;
  }
  .modal-input:focus { outline: none; border-color: #3b82f6; }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .modal-cancel {
    background: transparent;
    border: 1px solid #374151;
    border-radius: 4px;
    color: #9ca3af;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 4px 12px;
  }
  .modal-cancel:hover { border-color: #6b7280; color: #f0f0f0; }
  .modal-submit {
    background: #14532d;
    border: 1px solid #22c55e;
    border-radius: 4px;
    color: #86efac;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 4px 12px;
  }
  .modal-submit:hover { background: #166534; }
</style>
