<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { runContainer } from '$lib/ipc';

  const dispatch = createEventDispatcher<{ done: void }>();

  let image = '';
  let nameInput = '';
  let cmdInput = '';

  let running = false;
  let error = '';

  onDestroy(() => {});

  async function run() {
    if (!image.trim()) return;
    error = '';
    running = true;
    try {
      const args = cmdInput.trim() ? cmdInput.trim().split(/\s+/) : [];
      const name = nameInput.trim() || null;
      // Always detach — the UI is for launching background containers.
      // Interactive / foreground sessions belong in a terminal.
      await runContainer(image.trim(), name, args, true);
      dispatch('done');
    } catch (e) {
      error = String(e);
      running = false;
    }
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
      placeholder="Command  (optional, e.g. sleep 60)"
      bind:value={cmdInput}
      disabled={running}
    />
    <button class="btn" on:click={run} disabled={running || !image.trim()}>
      {running ? '…' : 'Run'}
    </button>
    <button class="btn ghost" on:click={() => dispatch('done')} disabled={running}>✕</button>
  </div>
  {#if error}<div class="err">{error}</div>{/if}
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
    gap: 8px;
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
  .err { color: #f87171; font-size: 0.8rem; }
</style>
