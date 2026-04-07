<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import '@xterm/xterm/css/xterm.css';

  let container: HTMLDivElement;
  let label = '';
  let exited = false;
  let exitCode: number | null = null;
  let unlisteners: UnlistenFn[] = [];

  let term: any = null;
  let fitAddon: any = null;
  let resizeObserver: ResizeObserver | null = null;

  onMount(async () => {
    const { Terminal } = await import('@xterm/xterm');
    const { FitAddon } = await import('@xterm/addon-fit');

    term = new Terminal({
      cursorBlink: true,
      fontFamily: 'Menlo, Monaco, "Cascadia Mono", "Courier New", monospace',
      fontSize: 13,
      theme: {
        background:          '#1e1e2e',
        foreground:          '#cdd6f4',
        cursor:              '#f5e0dc',
        selectionBackground: '#585b70',
        black:   '#45475a', red:     '#f38ba8', green:   '#a6e3a1', yellow: '#f9e2af',
        blue:    '#89b4fa', magenta: '#f5c2e7', cyan:    '#94e2d5', white:  '#bac2de',
        brightBlack:   '#585b70', brightRed:   '#f38ba8', brightGreen: '#a6e3a1',
        brightYellow:  '#f9e2af', brightBlue:  '#89b4fa', brightMagenta: '#f5c2e7',
        brightCyan:    '#94e2d5', brightWhite: '#a6adc8',
      },
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);

    // setTimeout is more reliable than requestAnimationFrame in a freshly
    // created WebviewWindow before the first paint completes.
    await new Promise<void>(r => setTimeout(r, 50));
    fitAddon.fit();
    term.focus();

    label = new URLSearchParams(window.location.search).get('label') ?? '';
    if (!label) {
      term.writeln('\x1b[31mError: no session label in URL.\x1b[0m');
      return;
    }

    const unlistenOutput = await listen<number[]>(`pty-output-${label}`, (event) => {
      term.write(new Uint8Array(event.payload));
    });

    const unlistenExit = await listen<number>(`pty-exit-${label}`, (event) => {
      exited = true;
      exitCode = event.payload ?? null;
      term.writeln(exitCode !== null
        ? `\r\n\x1b[90m[Process exited with code ${exitCode}]\x1b[0m`
        : '\r\n\x1b[90m[Session ended]\x1b[0m');
    });

    unlisteners = [unlistenOutput, unlistenExit];

    term.onData((data: string) => {
      const bytes = Array.from(new TextEncoder().encode(data));
      invoke('pty_input', { label, data: bytes }).catch((e) => {
        term?.writeln(`\r\n\x1b[31m[pty_input error: ${e}]\x1b[0m`);
      });
    });

    resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
      if (term) invoke('pty_resize', { label, cols: term.cols, rows: term.rows }).catch(() => {});
    });
    resizeObserver.observe(container);

    try {
      await invoke('pty_start', { label });
      term.focus();
    } catch (e) {
      term.writeln(`\x1b[31m[pty_start failed: ${e}]\x1b[0m`);
    }
  });

  onDestroy(async () => {
    resizeObserver?.disconnect();
    for (const u of unlisteners) u();
    term?.dispose();
    if (label) await invoke('pty_close', { label }).catch(() => {});
  });
</script>

<div class="term" bind:this={container}></div>

{#if exited}
  <div class="exit-overlay">
    Session ended{exitCode !== null ? ` (exit ${exitCode})` : ''}.
    Press <kbd>Cmd+W</kbd> to close.
  </div>
{/if}

<style>
  :global(*, *::before, *::after) { box-sizing: border-box; }
  :global(html, body) {
    margin: 0; padding: 0;
    width: 100%; height: 100%;
    background: #1e1e2e;
    overflow: hidden;
  }
  .term { width: 100vw; height: 100vh; }
  .exit-overlay {
    position: fixed; bottom: 12px; left: 50%; transform: translateX(-50%);
    background: rgba(30,30,46,0.92); border: 1px solid #45475a; border-radius: 6px;
    color: #a6adc8; font-family: system-ui,-apple-system,sans-serif; font-size: .8rem;
    padding: 6px 16px; pointer-events: none; white-space: nowrap;
  }
  kbd { background: #313244; border-radius: 3px; padding: 0 4px; font-family: inherit; }
</style>
