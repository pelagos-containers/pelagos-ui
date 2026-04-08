<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { listImages, pullImage, runContainer, launchTerminalWindow, removeImage } from '$lib/ipc';
  import type { ImageInfo } from '$lib/ipc';

  // ── image list ─────────────────────────────────────────────────────────────
  let images: ImageInfo[] = [];
  let loadError = '';
  let loadingImages = true;

  // ── search / filter ────────────────────────────────────────────────────────
  let query = '';

  // ── selected entry + run form ──────────────────────────────────────────────
  let selectedRef: string | null = null;
  let nameInput = '';
  let cmdInput = '';
  let portsInput = '';
  let volumesInput = '';
  let mode: 'background' | 'interactive' = 'background';

  // ── run/pull progress ──────────────────────────────────────────────────────
  let busy = false;
  let runLog: string[] = [];
  let runError = '';

  // ── remove confirm ─────────────────────────────────────────────────────────
  let confirmRef: string | null = null;
  let removing = false;
  let removeError = '';

  onMount(() => { load(); });

  async function load() {
    loadingImages = true;
    loadError = '';
    try {
      images = await listImages();
    } catch (e) {
      loadError = String(e);
    } finally {
      loadingImages = false;
    }
  }

  // ── derived: filter logic ──────────────────────────────────────────────────
  $: q = query.trim().toLowerCase();
  $: matchedImages = q
    ? images.filter(img => img.reference.toLowerCase().includes(q))
    : [...images];
  $: exactMatch = images.some(img => img.reference.toLowerCase() === q);
  $: showNewEntry = q !== '' && !exactMatch;

  // ── select / deselect ─────────────────────────────────────────────────────
  function select(ref: string) {
    if (selectedRef === ref) { selectedRef = null; return; }
    selectedRef = ref;
    nameInput = '';
    cmdInput = '';
    portsInput = '';
    volumesInput = '';
    mode = 'background';
    runLog = [];
    runError = '';
  }

  // ── run (pull-then-run if image not local) ─────────────────────────────────
  async function run() {
    if (!selectedRef || busy) return;
    busy = true;
    runLog = [];
    runError = '';

    const isLocal = images.some(img => img.reference === selectedRef);

    if (!isLocal) {
      const unlistenPull = await listen<string>('pull-log', (ev) => {
        // Suppress internal Rust/library log lines.
        if (/^\[\d{4}-\d{2}-\d{2}T/.test(ev.payload)) return;
        runLog = [...runLog, ev.payload];
      });
      try {
        const code = await pullImage(selectedRef);
        if (code !== 0) {
          const errLine = [...runLog].reverse().find(l => l.includes('error:'));
          runError = errLine ?? `pull exited with code ${code}`;
          busy = false;
          unlistenPull();
          return;
        }
        await load();
      } catch (e) {
        runError = String(e);
        busy = false;
        unlistenPull();
        return;
      }
      unlistenPull();
    }

    const args = cmdInput.trim() ? cmdInput.trim().split(/\s+/) : [];
    const name = nameInput.trim() || null;
    const ports = portsInput.trim() ? portsInput.trim().split(/[\s,]+/) : [];
    const volumes = volumesInput.trim() ? volumesInput.trim().split(/[\s,]+/) : [];

    try {
      if (mode === 'interactive') {
        await launchTerminalWindow(selectedRef, name, args, ports, volumes);
        selectedRef = null;
        query = '';
      } else {
        const unlistenRun = await listen<string>('run-log', (ev) => {
          runLog = [...runLog, ev.payload];
        });
        await runContainer(selectedRef, name, args, ports, volumes);
        unlistenRun();
        selectedRef = null;
        query = '';
      }
    } catch (e) {
      runError = String(e);
    } finally {
      busy = false;
    }
  }

  // ── remove ─────────────────────────────────────────────────────────────────
  async function doRemove() {
    if (!confirmRef) return;
    removing = true;
    removeError = '';
    try {
      await removeImage(confirmRef);
      if (selectedRef === confirmRef) selectedRef = null;
      confirmRef = null;
      await load();
    } catch (e) {
      removeError = String(e);
    } finally {
      removing = false;
    }
  }

  export function shortDigest(digest: string): string {
    const hex = digest.startsWith('sha256:') ? digest.slice(7) : digest;
    return hex.slice(0, 12);
  }
</script>

<!-- ── search row ─────────────────────────────────────────────────────────── -->
<div class="search-row">
  <span class="search-icon" aria-hidden="true">⌕</span>
  <input
    class="search"
    placeholder="Image name or reference…  (e.g. alpine:latest)"
    bind:value={query}
    disabled={busy}
    on:keydown={(e) => {
      if (e.key !== 'Enter') return;
      const target = showNewEntry ? query.trim() : (matchedImages[0]?.reference ?? null);
      if (target) select(target);
    }}
  />
  <button class="btn ghost sm" on:click={() => load()} disabled={loadingImages || busy} title="Refresh">↺</button>
</div>

{#if loadError}
  <p class="err load-err">{loadError}</p>
{/if}

<!-- ── image list ─────────────────────────────────────────────────────────── -->
<div class="image-list">

  <!-- new / unmatched entry -->
  {#if showNewEntry}
    {@const newRef = query.trim()}
    <div
      class="image-row new-entry"
      class:selected={selectedRef === newRef}
      role="button"
      tabindex="0"
      on:click={() => select(newRef)}
      on:keydown={(e) => e.key === 'Enter' && select(newRef)}
    >
      <span class="ref">{newRef}</span>
      <span class="pull-badge">pull &amp; run</span>
    </div>

    {#if selectedRef === newRef}
      <div class="expansion">
        <div class="form-row">
          <input class="input" placeholder="Name  (optional)" bind:value={nameInput} disabled={busy} />
          <input
            class="input wide"
            placeholder={mode === 'interactive' ? 'Command  (e.g. /bin/sh)' : 'Command  (e.g. sleep 60)'}
            bind:value={cmdInput}
            disabled={busy}
            on:keydown={(e) => e.key === 'Enter' && run()}
          />
          <div class="seg" role="group">
            <button class="seg-btn" class:active={mode === 'background'}
              on:click={() => mode = 'background'} disabled={busy}>Background</button>
            <button class="seg-btn" class:active={mode === 'interactive'}
              on:click={() => { if (!cmdInput.trim()) cmdInput = '/bin/sh'; mode = 'interactive'; }}
              disabled={busy}>Interactive</button>
          </div>
          <button class="btn run-btn" on:click={run} disabled={busy || !newRef}>
            {busy ? '…' : mode === 'interactive' ? 'Open terminal' : 'Run'}
          </button>
        </div>
        <div class="form-row">
          <input class="input" placeholder="Ports  (e.g. 8080:80)" bind:value={portsInput} disabled={busy} />
          <input class="input wide" placeholder="Volumes  (e.g. ~/Projects/mysite:/srv)" bind:value={volumesInput} disabled={busy} />
        </div>
        {#if runLog.length > 0}
          <div class="log">{#each runLog as line}<div>{line}</div>{/each}</div>
        {/if}
        {#if runError}<div class="err">{runError}</div>{/if}
      </div>
    {/if}
  {/if}

  <!-- local images -->
  {#if loadingImages}
    <p class="hint">Loading…</p>
  {:else if matchedImages.length === 0 && !showNewEntry}
    <p class="hint">{q ? 'No matching local images.' : 'No images cached locally.'}</p>
  {:else}
    {#each matchedImages as img (img.reference)}
      <div
        class="image-row"
        class:selected={selectedRef === img.reference}
        role="button"
        tabindex="0"
        on:click={() => select(img.reference)}
        on:keydown={(e) => e.key === 'Enter' && select(img.reference)}
      >
        <span class="ref">{img.reference}</span>
        <span class="meta">{shortDigest(img.digest)}</span>
        <span class="meta layers">{img.layers.length} layers</span>
        <button
          class="btn ghost sm remove-btn"
          on:click|stopPropagation={() => { confirmRef = img.reference; removeError = ''; }}
          disabled={busy}
        >Remove</button>
      </div>

      {#if selectedRef === img.reference}
        <div class="expansion">
          <div class="form-row">
            <input class="input" placeholder="Name  (optional)" bind:value={nameInput} disabled={busy} />
            <input
              class="input wide"
              placeholder={mode === 'interactive' ? 'Command  (e.g. /bin/sh)' : 'Command  (e.g. sleep 60)'}
              bind:value={cmdInput}
              disabled={busy}
              on:keydown={(e) => e.key === 'Enter' && run()}
            />
            <div class="seg" role="group">
              <button class="seg-btn" class:active={mode === 'background'}
                on:click={() => mode = 'background'} disabled={busy}>Background</button>
              <button class="seg-btn" class:active={mode === 'interactive'}
                on:click={() => { if (!cmdInput.trim()) cmdInput = '/bin/sh'; mode = 'interactive'; }}
                disabled={busy}>Interactive</button>
            </div>
            <button class="btn run-btn" on:click={run} disabled={busy}>
              {busy ? '…' : mode === 'interactive' ? 'Open terminal' : 'Run'}
            </button>
          </div>
          <div class="form-row">
            <input class="input" placeholder="Ports  (e.g. 8080:80)" bind:value={portsInput} disabled={busy} />
            <input class="input wide" placeholder="Volumes  (e.g. ~/Projects/mysite:/srv)" bind:value={volumesInput} disabled={busy} />
          </div>
          {#if runLog.length > 0}
            <div class="log">{#each runLog as line}<div>{line}</div>{/each}</div>
          {/if}
          {#if runError}<div class="err">{runError}</div>{/if}
        </div>
      {/if}
    {/each}
  {/if}

</div>

<!-- ── remove confirm dialog ──────────────────────────────────────────────── -->
{#if confirmRef !== null}
  <div class="overlay">
    <div class="dialog">
      <p>Remove image <strong>{confirmRef}</strong>?</p>
      {#if removeError}<p class="err">{removeError}</p>{/if}
      <div class="dialog-btns">
        <button class="btn danger" on:click={doRemove} disabled={removing}>
          {removing ? 'Removing…' : 'Remove'}
        </button>
        <button class="btn ghost" on:click={() => { confirmRef = null; removeError = ''; }} disabled={removing}>Cancel</button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── search row ─────────────────────────────────────────────────────────── */
  .search-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 24px 8px;
    border-bottom: 1px solid #1f2937;
  }
  .search-icon { color: #6b7280; font-size: 1rem; flex-shrink: 0; }
  .search {
    flex: 1;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 4px;
    color: #f0f0f0;
    font-size: 0.85rem;
    padding: 5px 10px;
  }
  .search:focus  { outline: none; border-color: #58a6ff; }
  .search:disabled { opacity: 0.5; }

  /* ── image list ─────────────────────────────────────────────────────────── */
  .image-list { padding: 0 24px 16px; }

  .hint { color: #6b7280; font-size: 0.8rem; margin: 12px 0; }
  .load-err { margin: 8px 0; }

  .image-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 7px 10px;
    border-radius: 4px;
    cursor: pointer;
    border-bottom: 1px solid #1a2130;
    user-select: none;
  }
  .image-row:last-of-type { border-bottom: none; }
  .image-row:hover  { background: #0d1117; }
  .image-row.selected { background: #111827; }
  .image-row.new-entry { border-left: 2px solid #3b82f6; padding-left: 8px; }

  .ref   { flex: 1; font-size: 0.82rem; color: #c9d1d9; word-break: break-all; }
  .meta  { font-family: monospace; font-size: 0.72rem; color: #6e7681; white-space: nowrap; }
  .layers { min-width: 56px; text-align: right; }
  .pull-badge {
    font-size: 0.68rem;
    background: #1d3a6b;
    border: 1px solid #3b82f6;
    border-radius: 10px;
    color: #93c5fd;
    padding: 1px 7px;
    white-space: nowrap;
  }
  .remove-btn { opacity: 0; }
  .image-row:hover .remove-btn,
  .image-row.selected .remove-btn { opacity: 1; }

  /* ── inline run form ────────────────────────────────────────────────────── */
  .expansion {
    background: #0d1117;
    border: 1px solid #21262d;
    border-radius: 4px;
    padding: 10px 12px;
    margin: 2px 0 8px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .form-row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .input {
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 4px;
    color: #f0f0f0;
    font-size: 0.8rem;
    padding: 4px 8px;
    min-width: 110px;
  }
  .input.wide { flex: 1; min-width: 160px; }
  .input:focus   { outline: none; border-color: #58a6ff; }
  .input:disabled { opacity: 0.5; }

  .log {
    max-height: 140px;
    overflow-y: auto;
    background: #010409;
    border: 1px solid #30363d;
    border-radius: 4px;
    padding: 6px 8px;
    font-family: monospace;
    font-size: 0.72rem;
    color: #8b949e;
    white-space: pre-wrap;
  }
  .err { color: #f87171; font-size: 0.8rem; margin: 2px 0; }

  /* ── seg control ────────────────────────────────────────────────────────── */
  .seg {
    display: flex;
    border: 1px solid #30363d;
    border-radius: 4px;
    overflow: hidden;
    flex-shrink: 0;
  }
  .seg-btn {
    background: transparent;
    border: none;
    color: #8b949e;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 4px 10px;
    white-space: nowrap;
  }
  .seg-btn + .seg-btn { border-left: 1px solid #30363d; }
  .seg-btn.active  { background: #21262d; color: #f0f0f0; }
  .seg-btn:hover:not(:disabled):not(.active) { background: #161b22; color: #f0f0f0; }
  .seg-btn:disabled { opacity: 0.5; cursor: default; }

  /* ── buttons ────────────────────────────────────────────────────────────── */
  .btn {
    background: #238636;
    border: none;
    border-radius: 4px;
    color: #fff;
    cursor: pointer;
    font-size: 0.8rem;
    padding: 4px 14px;
    white-space: nowrap;
  }
  .btn:hover:not(:disabled)  { background: #2ea043; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn.ghost {
    background: transparent;
    border: 1px solid #30363d;
    color: #8b949e;
    padding: 4px 8px;
  }
  .btn.ghost:hover:not(:disabled) { border-color: #8b949e; color: #f0f0f0; }
  .btn.sm   { font-size: 0.72rem; padding: 2px 8px; }
  .btn.danger { background: #b91c1c; }
  .btn.danger:hover:not(:disabled) { background: #dc2626; }
  .run-btn  { flex-shrink: 0; }

  /* ── confirm overlay ────────────────────────────────────────────────────── */
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .dialog {
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 20px 24px;
    min-width: 280px;
    max-width: 420px;
  }
  .dialog p     { margin: 0 0 12px; font-size: 0.85rem; }
  .dialog-btns  { display: flex; gap: 8px; justify-content: flex-end; }
</style>
