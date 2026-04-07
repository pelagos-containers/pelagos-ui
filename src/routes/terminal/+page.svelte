<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';

  let container: HTMLDivElement;
  let label = '';
  let exited = false;
  let exitCode: number | null = null;
  let unlisteners: UnlistenFn[] = [];

  // xterm instances — typed as any to avoid SSR import issues.
  let term: any = null;
  let fitAddon: any = null;
  let resizeObserver: ResizeObserver | null = null;

  onMount(async () => {
    // Read the window label from the URL — injected by launch_terminal_window.
    label = new URLSearchParams(window.location.search).get('label') ?? '';
    if (!label) {
      console.error('terminal: no label in URL');
      return;
    }

    // Dynamic import keeps xterm out of the SSR/prerender bundle.
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
        black:               '#45475a',
        red:                 '#f38ba8',
        green:               '#a6e3a1',
        yellow:              '#f9e2af',
        blue:                '#89b4fa',
        magenta:             '#f5c2e7',
        cyan:                '#94e2d5',
        white:               '#bac2de',
        brightBlack:         '#585b70',
        brightRed:           '#f38ba8',
        brightGreen:         '#a6e3a1',
        brightYellow:        '#f9e2af',
        brightBlue:          '#89b4fa',
        brightMagenta:       '#f5c2e7',
        brightCyan:          '#94e2d5',
        brightWhite:         '#a6adc8',
      },
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(container);
    fitAddon.fit();

    // Bridge PTY output → xterm.
    const unlistenOutput = await listen<number[]>(`pty-output-${label}`, (event) => {
      term.write(new Uint8Array(event.payload));
    });

    // Handle process exit.
    const unlistenExit = await listen<number>(`pty-exit-${label}`, (event) => {
      exited = true;
      exitCode = event.payload ?? null;
      const msg = exitCode !== null
        ? `\r\n\x1b[90m[Process exited with code ${exitCode}]\x1b[0m`
        : '\r\n\x1b[90m[Session ended]\x1b[0m';
      term.writeln(msg);
    });

    unlisteners = [unlistenOutput, unlistenExit];

    // Bridge xterm keyboard input → PTY.
    term.onData((data: string) => {
      const bytes = Array.from(new TextEncoder().encode(data));
      invoke('pty_input', { label, data: bytes }).catch(() => {});
    });

    // Resize the PTY whenever the container changes size.
    resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
      if (term) {
        invoke('pty_resize', { label, cols: term.cols, rows: term.rows }).catch(() => {});
      }
    });
    resizeObserver.observe(container);

    // Start the PTY.  Do this last so the listeners are in place before any
    // output arrives.
    try {
      await invoke('pty_start', { label });
    } catch (e) {
      term.writeln(`\r\n\x1b[31mError starting session: ${e}\x1b[0m`);
    }
  });

  onDestroy(async () => {
    resizeObserver?.disconnect();
    for (const u of unlisteners) u();
    term?.dispose();
    if (label) {
      await invoke('pty_close', { label }).catch(() => {});
    }
  });
</script>

<!-- Full-window terminal — no body padding, no scrollbars. -->
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
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    background: #1e1e2e;
    overflow: hidden;
  }

  .term {
    width: 100vw;
    height: 100vh;
  }

  /* xterm.js injects its own CSS — we just need the container to fill the window. */
  :global(.xterm) { height: 100%; }
  :global(.xterm-viewport) { overflow: hidden !important; }

  .exit-overlay {
    position: fixed;
    bottom: 12px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(30, 30, 46, 0.92);
    border: 1px solid #45475a;
    border-radius: 6px;
    color: #a6adc8;
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 0.8rem;
    padding: 6px 16px;
    pointer-events: none;
    white-space: nowrap;
  }

  kbd {
    background: #313244;
    border-radius: 3px;
    padding: 0 4px;
    font-family: inherit;
  }
</style>
