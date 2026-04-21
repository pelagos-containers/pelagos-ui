<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { listImages, pullImage, removeImage } from '$lib/ipc';
  import type { ImageInfo } from '$lib/ipc';

  const dispatch = createEventDispatcher<{ close: void; run: string }>();

  let images: ImageInfo[] = [];
  let loadError = '';
  let loading = true;

  // Pull dialog state
  let showPull = false;
  let pullRef = '';
  let pulling = false;
  let pullLog: string[] = [];
  let pullError = '';

  // Remove confirm state
  let confirmRef: string | null = null;
  let removing = false;
  let removeError = '';

  onMount(() => {
    load();
  });

  async function load() {
    loading = true;
    loadError = '';
    try {
      images = await listImages();
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  async function startPull() {
    if (!pullRef.trim()) return;
    pulling = true;
    pullLog = [];
    pullError = '';

    const unlisten = await listen<string>('pull-log', (ev) => {
      pullLog = [...pullLog, ev.payload];
    });

    try {
      const code = await pullImage(pullRef.trim().toLowerCase());
      if (code !== 0) {
        pullError = `pull exited with code ${code}`;
      } else {
        showPull = false;
        pullRef = '';
        await load();
      }
    } catch (e) {
      pullError = String(e);
    } finally {
      pulling = false;
      unlisten();
    }
  }

  async function confirmRemove(ref: string) {
    confirmRef = ref;
    removeError = '';
  }

  async function doRemove() {
    if (!confirmRef) return;
    removing = true;
    removeError = '';
    try {
      await removeImage(confirmRef);
      confirmRef = null;
      await load();
    } catch (e) {
      removeError = String(e);
    } finally {
      removing = false;
    }
  }

  function shortDigest(digest: string): string {
    const hex = digest.startsWith('sha256:') ? digest.slice(7) : digest;
    return hex.slice(0, 12);
  }
</script>

<div class="view">
  <div class="toolbar">
    <span class="title">Images</span>
    <button class="btn" on:click={() => (showPull = !showPull)} disabled={pulling}>+ Pull</button>
    <button class="btn ghost" on:click={() => load()} disabled={loading}>↺</button>
    <button class="btn ghost" on:click={() => dispatch('close')}>✕</button>
  </div>

  {#if showPull}
    <div class="pull-panel">
      <input
        class="input"
        placeholder="Image reference  (e.g. alpine:latest)"
        bind:value={pullRef}
        disabled={pulling}
        on:keydown={(e) => e.key === 'Enter' && startPull()}
      />
      <button class="btn" on:click={startPull} disabled={pulling || !pullRef.trim()}>
        {pulling ? 'Pulling…' : 'Pull'}
      </button>
      <button class="btn ghost" on:click={() => { showPull = false; pullRef = ''; pullLog = []; pullError = ''; }} disabled={pulling}>✕</button>
      {#if pullLog.length > 0}
        <div class="log">
          {#each pullLog as line}<div>{line}</div>{/each}
        </div>
      {/if}
      {#if pullError}<div class="err">{pullError}</div>{/if}
    </div>
  {/if}

  {#if loading}
    <p class="hint">Loading…</p>
  {:else if loadError}
    <p class="err">{loadError}</p>
  {:else if images.length === 0}
    <p class="hint">No images cached locally.  Use Pull to fetch one.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Reference</th>
          <th>Digest</th>
          <th>Layers</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each images as img (img.reference)}
          <tr>
            <td class="ref">{img.reference}</td>
            <td class="mono">{shortDigest(img.digest)}</td>
            <td class="center">{img.layers.length}</td>
            <td class="actions">
              <button class="btn-sm" on:click={() => dispatch('run', img.reference)}>▶ Run</button>
              <button class="btn-sm danger" on:click={() => confirmRemove(img.reference)}>Remove</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

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
  .view {
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 12px 16px;
    margin-bottom: 16px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }
  .title { font-size: 0.85rem; font-weight: 600; flex: 1; }

  .pull-panel {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    flex-wrap: wrap;
    margin-bottom: 12px;
    padding: 10px 12px;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 4px;
  }
  .input {
    flex: 1;
    min-width: 220px;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 4px;
    color: #f0f0f0;
    font-size: 0.8rem;
    padding: 4px 8px;
  }
  .input:focus { outline: none; border-color: #58a6ff; }
  .input:disabled { opacity: 0.5; }

  .log {
    width: 100%;
    max-height: 160px;
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

  table { width: 100%; border-collapse: collapse; }
  th {
    text-align: left;
    padding: 4px 10px;
    color: #6b7280;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    border-bottom: 1px solid #1f2937;
  }
  td {
    padding: 6px 10px;
    font-size: 0.8rem;
    border-bottom: 1px solid #1a2130;
    vertical-align: middle;
  }
  td.ref  { color: #c9d1d9; word-break: break-all; }
  td.mono { font-family: monospace; color: #6e7681; font-size: 0.75rem; }
  td.center { text-align: center; color: #6e7681; }
  td.actions { text-align: right; white-space: nowrap; }
  tr:last-child td { border-bottom: none; }
  tr:hover td { background: #0d1117; }

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
  .btn:hover:not(:disabled) { background: #2ea043; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn.ghost {
    background: transparent;
    border: 1px solid #30363d;
    color: #8b949e;
    padding: 4px 8px;
  }
  .btn.ghost:hover:not(:disabled) { border-color: #8b949e; color: #f0f0f0; }
  .btn.danger { background: #b91c1c; }
  .btn.danger:hover:not(:disabled) { background: #dc2626; }

  .btn-sm {
    background: transparent;
    border: 1px solid #6b1d1d;
    border-radius: 4px;
    color: #f87171;
    cursor: pointer;
    font-size: 0.72rem;
    padding: 2px 8px;
    white-space: nowrap;
  }
  .btn-sm:hover { background: #2d1b1b; }
  .btn-sm.danger { color: #f87171; }

  .hint { color: #6b7280; font-size: 0.8rem; margin: 12px 0; }
  .err  { color: #f87171; font-size: 0.8rem; margin: 4px 0; }

  /* Confirm overlay */
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
    max-width: 400px;
  }
  .dialog p { margin: 0 0 12px; font-size: 0.85rem; }
  .dialog-btns { display: flex; gap: 8px; justify-content: flex-end; }
</style>
