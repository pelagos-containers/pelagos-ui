<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { runContainer, launchInteractive } from '$lib/ipc';

  const dispatch = createEventDispatcher<{ done: void }>();

  let image = '';
  let nameInput = '';
  let cmdInput = '';
  let mode: 'background' | 'interactive' = 'background';

  let running = false;
  let error = '';

  async function run() {
    if (!image.trim()) return;
    error = '';
    running = true;
    try {
      const args = cmdInput.trim() ? cmdInput.trim().split(/\s+/) : [];
      const name = nameInput.trim() || null;
      if (mode === 'interactive') {
        await launchInteractive(image.trim(), name, args);
        // Terminal window opened — close panel, container will appear once it starts
        dispatch('done');
      } else {
        await runContainer(image.trim(), name, args);
        dispatch('done');
      }
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
      placeholder={mode === 'interactive' ? 'Command  (e.g. /bin/sh)' : 'Command  (e.g. sleep 60)'}
      bind:value={cmdInput}
      disabled={running}
    />

    <div class="seg" role="group">
      <button
        class="seg-btn"
        class:active={mode === 'background'}
        on:click={() => (mode = 'background')}
        disabled={running}
      >Background</button>
      <button
        class="seg-btn"
        class:active={mode === 'interactive'}
        on:click={() => (mode = 'interactive')}
        disabled={running}
      >Interactive</button>
    </div>

    <button class="btn" on:click={run} disabled={running || !image.trim()}>
      {running ? '…' : mode === 'interactive' ? 'Open terminal' : 'Run'}
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

  .seg {
    display: flex;
    border: 1px solid #30363d;
    border-radius: 4px;
    overflow: hidden;
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
  .seg-btn.active { background: #21262d; color: #f0f0f0; }
  .seg-btn:hover:not(:disabled):not(.active) { background: #161b22; color: #f0f0f0; }
  .seg-btn:disabled { opacity: 0.5; cursor: default; }

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
