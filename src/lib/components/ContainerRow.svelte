<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import StatusBadge from './StatusBadge.svelte';
  import type { ContainerInfo } from '../ipc';

  export let container: ContainerInfo;
  export let editMode = false;
  export let checked = false;

  const dispatch = createEventDispatcher<{ stop: string; remove: string; toggle: string; logs: string }>();

  function age(iso: string): string {
    const s = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
    if (s < 60)   return `${s}s`;
    if (s < 3600) return `${Math.floor(s / 60)}m`;
    if (s < 86400) return `${Math.floor(s / 3600)}h`;
    return `${Math.floor(s / 86400)}d`;
  }

  const MAX_CMD = 30;
  $: cmdDisplay = (() => {
    const s = container.command.join(' ');
    return s.length > MAX_CMD ? s.slice(0, MAX_CMD - 1) + '…' : s;
  })();

  $: selectable = editMode && container.status === 'exited';
  $: muted = editMode && container.status === 'running';
</script>

<tr class:muted class:selectable>
  <td class="check-col">
    {#if selectable}
      <input
        type="checkbox"
        checked={checked}
        on:change={() => dispatch('toggle', container.name)}
      />
    {/if}
  </td>
  <td class="name">{container.name}</td>
  <td><StatusBadge status={container.status} /></td>
  <td class="mono">{container.image ?? container.rootfs}</td>
  <td class="mono dim" title={container.command.join(' ')}>{cmdDisplay}</td>
  <td class="dim">{age(container.started_at)}</td>
  <td class="pid dim">{container.pid > 0 ? container.pid : '—'}</td>
  <td class="actions">
    {#if !editMode}
      <button class="logs-btn" on:click={() => dispatch('logs', container.name)}>Logs</button>
      {#if container.status === 'running'}
        <button on:click={() => dispatch('stop', container.name)}>Stop</button>
      {/if}
      <button class="danger" on:click={() => dispatch('remove', container.name)}>Remove</button>
    {:else if container.status === 'running'}
      <button on:click={() => dispatch('stop', container.name)}>Stop</button>
    {/if}
  </td>
</tr>

<style>
  td { padding: 9px 12px; vertical-align: middle; border-bottom: 1px solid #1a1f2e; }
  .mono   { font-family: ui-monospace, monospace; font-size: 0.82em; }
  .dim    { color: #6b7280; }
  .name   { font-weight: 600; }
  .pid    { text-align: right; font-family: ui-monospace, monospace; font-size: 0.82em; }
  .actions { display: flex; gap: 6px; justify-content: flex-end; }

  .check-col {
    width: 32px;
    padding: 9px 4px 9px 12px;
  }
  .check-col input[type="checkbox"] {
    cursor: pointer;
    width: 15px;
    height: 15px;
    accent-color: #3b82f6;
  }

  tr.muted td { opacity: 0.35; }
  tr.selectable { cursor: pointer; }
  tr.selectable:hover td { background: #0d1117; }

  button {
    padding: 3px 10px;
    border-radius: 4px;
    border: 1px solid #2d3748;
    background: #1a1f2e;
    color: #d1d5db;
    cursor: pointer;
    font-size: 0.75rem;
  }
  button:hover        { background: #2d3748; }
  button.danger:hover  { background: #7f1d1d; border-color: #991b1b; color: #fca5a5; }
  button.logs-btn      { color: #93c5fd; border-color: #1d3a5c; }
  button.logs-btn:hover { background: #1d3a5c; border-color: #3b82f6; }
</style>
