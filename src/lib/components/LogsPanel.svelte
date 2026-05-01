<script lang="ts">
  import { onDestroy, onMount, tick, createEventDispatcher } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { streamLogs, stopLogs } from '$lib/ipc';

  export let containerName: string;
  export let isRunning: boolean = false;

  const dispatch = createEventDispatcher<{ close: void }>();

  let lines: string[] = [];
  let follow = isRunning;
  let logEl: HTMLDivElement;
  let unlisten: (() => void) | undefined;
  let error: string | null = null;

  // Strip ANSI escape sequences for plain-text rendering.
  // eslint-disable-next-line no-control-regex
  const ANSI_RE = /\x1b\[[0-9;]*[mGKHFJA-Z]/g;
  function stripAnsi(s: string): string {
    return s.replace(ANSI_RE, '');
  }

  async function scrollToBottom() {
    if (follow && logEl) {
      await tick();
      logEl.scrollTop = logEl.scrollHeight;
    }
  }

  async function startStream() {
    lines = [];
    error = null;
    if (unlisten) { unlisten(); unlisten = undefined; }

    unlisten = await listen<{ name: string; line: string }>('log-line', ({ payload }) => {
      if (payload.name !== containerName) return;
      lines = [...lines, stripAnsi(payload.line)];
      scrollToBottom();
    });

    try {
      await streamLogs(containerName, follow);
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleFollow() {
    follow = !follow;
    await stopLogs(containerName);
    await startStream();
    if (follow) scrollToBottom();
  }

  function handleClose() {
    stopLogs(containerName);
    dispatch('close');
  }

  onMount(startStream);
  onDestroy(() => {
    stopLogs(containerName).catch(() => {});
    unlisten?.();
  });
</script>

<div class="logs-panel">
  <div class="logs-header">
    <span class="logs-title">{containerName}</span>
    <span class="logs-label">Logs</span>
    <div class="logs-actions">
      <button
        class="follow-btn"
        class:active={follow}
        on:click={toggleFollow}
        title={follow ? 'Following — click to stop' : 'Not following — click to follow'}
      >Follow</button>
      <button class="close-btn" on:click={handleClose} title="Close logs">×</button>
    </div>
  </div>

  {#if error}
    <div class="logs-error">{error}</div>
  {/if}

  <div class="logs-output" bind:this={logEl}>
    {#if lines.length === 0 && !error}
      <span class="empty">No log output.</span>
    {:else}
      {#each lines as line, i (i)}
        <div class="log-line">{line}</div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .logs-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .logs-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 24px;
    flex-shrink: 0;
    border-bottom: 1px solid #1f2937;
  }

  .logs-title {
    font-weight: 600;
    font-size: 0.85rem;
  }

  .logs-label {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #6b7280;
  }

  .logs-actions {
    margin-left: auto;
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .follow-btn {
    background: transparent;
    border: 1px solid #374151;
    border-radius: 4px;
    color: #9ca3af;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 2px 10px;
  }
  .follow-btn:hover  { border-color: #6b7280; color: #f0f0f0; }
  .follow-btn.active { border-color: #3b82f6; color: #93c5fd; }

  .close-btn {
    background: transparent;
    border: none;
    color: #6b7280;
    cursor: pointer;
    font-size: 1.1rem;
    line-height: 1;
    padding: 0 4px;
  }
  .close-btn:hover { color: #f0f0f0; }

  .logs-error {
    padding: 6px 24px;
    font-size: 0.78rem;
    color: #f87171;
    flex-shrink: 0;
  }

  .logs-output {
    flex: 1;
    overflow-y: auto;
    padding: 8px 24px;
    font-family: ui-monospace, monospace;
    font-size: 0.78rem;
    line-height: 1.55;
  }

  .log-line {
    white-space: pre-wrap;
    word-break: break-all;
  }

  .empty {
    color: #6b7280;
  }
</style>
