<script lang="ts">
  export let status: 'running' | 'exited';
</script>

<span class="badge" class:running={status === 'running'} class:exited={status === 'exited'}>
  {status}
</span>

<style>
  .badge   { font-size: 0.7rem; padding: 2px 7px; border-radius: 9999px; font-weight: 600; }
  .running { background: #14532d; color: #86efac; }
  .exited  { background: #1f2937; color: #6b7280; }
</style>
