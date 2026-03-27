<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { runContainer } from '$lib/ipc';

  const dispatch = createEventDispatcher<{ done: void }>();

  let image = '';
  let nameInput = '';
  let cmdInput = '';
  let detach = true;

  let running = false;
  let log: string[] = [];
  let logEl: HTMLElement;
  let unlisten: UnlistenFn | null = null;
  let error = '';

  onDestroy(() => unlisten?.());

  async function run() {
    if (!image.trim()) return;
    error = '';
    log = [];
    running = true;

    unlisten = await listen<string>('run-log', (e) => {
      log = [...log, e.payload];
      // scroll to bottom
      requestAnimationFrame(() => {
        if (logEl) logEl.scrollTop = logEl.scrollHeight;
      });
    });

    try {
      const args = cmdInput.trim() ? cmdInput.trim().split(/\s+/) : [];
      const name = nameInput.trim() || null;
      const code = await runContainer(image.trim(), name, args, detach);
      if (code !== 0) {
        error = `exited with code ${code}`;
      } else if (!detach) {
        log = [...log, `[exited 0]`];
      }
      if (detach) dispatch('done');
    } catch (e) {
      error = String(e);
    } finally {
      unlisten?.();
      unlisten = null;
      running = false;
    }
  }

  function close() {
    dispatch('done');
  }
</script>

<div class="panel">
  <div class="row">
    <input
      class="input wide"
      placeholder="Image  (e.g. alpine:latest)"
      bind:value={image}
      disabled={running}
      on:keydown={(e) => e.key === 'Enter' && run()}
    />
    <input
      class="input"
      placeholder="Name  (optional)"
      bind:value={nameInput}
      disabled={running}
    />
    <input
      class="input wide"
      placeholder="Command  (optional, e.g. /bin/sh -c 'sleep 30')"
      bind:value={cmdInput}
      disabled={running}
    />
    <label class="check">
      <input type="checkbox" bind:checked={detach} disabled={running} />
      detach
    </label>
    <button class="btn" on:click={run} disabled={running || !image.trim()}>
      {running ? '…' : 'Run'}
    </button>
    <button class="btn ghost" on:click={close} disabled={running}>✕</button>
  </div>

  {#if log.length > 0 || error}
    <div class="log" bind:this={logEl}>
      {#each log as line}<div>{line}</div>{/each}
      {#if error}<div class="err">{error}</div>{/if}
    </div>
  {/if}
</div>

<style>
  .panel {
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 12px 16px;
    margin-bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .input {
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 4px;
    color: #f0f0f0;
    font-size: 0.8rem;
    padding: 4px 8px;
    min-width: 120px;
  }
  .input.wide { flex: 1; min-width: 180px; }
  .input:focus { outline: none; border-color: #58a6ff; }
  .input:disabled { opacity: 0.5; }
  .check {
    display: flex;
    align-items: center;
    gap: 4px;
    color: #8b949e;
    font-size: 0.8rem;
    white-space: nowrap;
  }
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
  .log {
    background: #0d1117;
    border: 1px solid #21262d;
    border-radius: 4px;
    color: #8b949e;
    font-family: monospace;
    font-size: 0.75rem;
    line-height: 1.5;
    max-height: 200px;
    overflow-y: auto;
    padding: 8px 10px;
  }
  .err { color: #f87171; }
</style>
