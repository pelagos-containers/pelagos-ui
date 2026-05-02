<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { kubernetesStatus, startKubernetes, stopKubernetes } from '$lib/ipc';

  let running = false;
  let busy = false;
  let error: string | null = null;
  let startLog: string[] = [];
  let unlisten: (() => void) | undefined;

  async function refresh() {
    try {
      running = await kubernetesStatus();
    } catch (e) {
      // Ignore status errors — runtime may not be reachable yet.
    }
  }

  async function toggle() {
    busy = true;
    error = null;
    startLog = [];
    try {
      if (running) {
        await stopKubernetes();
      } else {
        await startKubernetes();
      }
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  onMount(async () => {
    unlisten = await listen<string>('kubernetes-start-log', ({ payload }) => {
      startLog = [...startLog, payload];
    });
    await refresh();
    // Poll every 10 s so the indicator stays current.
    const iv = setInterval(refresh, 10_000);
    return () => clearInterval(iv);
  });

  onDestroy(() => unlisten?.());
</script>

<div class="k8s-pane">
  <div class="k8s-header">
    <div class="k8s-status">
      <span class="dot" class:dot-on={running} class:dot-off={!running}></span>
      <span class="status-label">{running ? 'Running' : 'Stopped'}</span>
    </div>
    <button
      class="toggle-btn"
      class:stop-btn={running}
      disabled={busy}
      on:click={toggle}
    >
      {#if busy}
        {running ? 'Stopping…' : 'Starting…'}
      {:else}
        {running ? 'Stop Kubernetes' : 'Start Kubernetes'}
      {/if}
    </button>
  </div>

  {#if error}
    <div class="k8s-error">{error}</div>
  {/if}

  {#if startLog.length > 0}
    <div class="start-log">
      {#each startLog as line}
        <div class="log-line">{line}</div>
      {/each}
    </div>
  {:else if !running}
    <p class="hint">
      Starts rusternetes (api-server + kubelet) and pelagos-dockerd inside the
      runtime environment.
    </p>
  {/if}
</div>

<style>
  .k8s-pane {
    padding: 16px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .k8s-header {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .k8s-status {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .dot-on  { background: #22c55e; box-shadow: 0 0 6px #22c55e88; }
  .dot-off { background: #4b5563; }

  .status-label {
    font-size: 0.85rem;
    font-weight: 600;
    color: #d1d5db;
    min-width: 60px;
  }

  .toggle-btn {
    background: #14532d;
    border: 1px solid #22c55e;
    border-radius: 4px;
    color: #86efac;
    cursor: pointer;
    font-size: 0.78rem;
    padding: 5px 14px;
  }
  .toggle-btn:hover:not(:disabled) { background: #166534; }
  .toggle-btn:disabled { opacity: 0.5; cursor: default; }
  .toggle-btn.stop-btn {
    background: #7f1d1d;
    border-color: #991b1b;
    color: #fca5a5;
  }
  .toggle-btn.stop-btn:hover:not(:disabled) { background: #991b1b; }

  .k8s-error {
    font-size: 0.78rem;
    color: #f87171;
  }

  .start-log {
    background: #0d1117;
    border: 1px solid #1f2937;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 0.75rem;
    line-height: 1.6;
    max-height: 160px;
    overflow-y: auto;
    padding: 8px 12px;
  }
  .log-line { white-space: pre-wrap; color: #9ca3af; }

  .hint {
    color: #6b7280;
    font-size: 0.78rem;
    margin: 0;
  }
</style>
