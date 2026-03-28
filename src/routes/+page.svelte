<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { containers, loading, error, startPolling } from '$lib/stores/containers';
  import ContainerRow from '$lib/components/ContainerRow.svelte';
  import RunPanel from '$lib/components/RunPanel.svelte';
  import ImagesView from '$lib/components/ImagesView.svelte';
  import { stopContainer, removeContainer } from '$lib/ipc';

  let stopPolling: () => void;
  let showRun = false;
  let showImages = false;
  let runPrefill = '';

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
</script>

<div class="app">
  <header>
    <h1>Pelagos</h1>
    {#if $loading}<span class="hint">loading…</span>{/if}
    {#if $error}<span class={$error === 'VM stopped' ? 'hint' : 'err'}>{$error}</span>{/if}
    <button class="run-btn" on:click={() => { runPrefill = ''; showRun = !showRun; showImages = false; }}>+ Run</button>
    <button class="run-btn" on:click={() => { showImages = !showImages; showRun = false; }}>Images</button>
  </header>

  {#if showRun}
    <RunPanel prefillImage={runPrefill} on:done={() => (showRun = false)} />
  {/if}

  {#if showImages}
    <ImagesView
      on:close={() => (showImages = false)}
      on:run={e => { runPrefill = e.detail; showImages = false; showRun = true; }}
    />
  {/if}

  {#if !$loading && $containers.length === 0}
    <p class="empty">No containers.  Run <code>pelagos run &lt;image&gt;</code> to start one.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th>Status</th>
          <th>Image</th>
          <th>Command</th>
          <th>Age</th>
          <th>PID</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each $containers as c (c.name)}
          <ContainerRow
            container={c}
            on:stop={e => handleStop(e.detail)}
            on:remove={e => handleRemove(e.detail)}
          />
        {/each}
      </tbody>
    </table>
  {/if}
</div>

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
  .app    { padding: 20px 24px; }
  header  { display: flex; align-items: baseline; gap: 16px; margin-bottom: 20px; }
  h1      { margin: 0; font-size: 1.1rem; font-weight: 700; letter-spacing: -0.01em; }
  .hint   { color: #6b7280; font-size: 0.8rem; }
  .err    { color: #f87171; font-size: 0.8rem; }
  .run-btn {
    margin-left: auto;
    background: #238636;
    border: none;
    border-radius: 4px;
    color: #fff;
    cursor: pointer;
    font-size: 0.8rem;
    padding: 3px 12px;
  }
  .run-btn:hover { background: #2ea043; }
  .empty  { color: #6b7280; margin-top: 40px; text-align: center; }
  code    { font-family: monospace; background: #1f2937; padding: 1px 5px; border-radius: 3px; }
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
  table   { width: 100%; border-collapse: collapse; }
  th {
    text-align: left;
    padding: 6px 12px;
    color: #6b7280;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    border-bottom: 1px solid #1f2937;
  }
</style>
