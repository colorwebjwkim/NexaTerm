<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import '@xterm/xterm/css/xterm.css'
  import { api, type Settings } from '../api'
  export interface TerminalHandle {
    write(data: number[], sequence: number): void
    fit(): void
    clear(): void
    focus(): void
  }
  let {
    connectionId,
    connected,
    settings,
    register,
    reportError,
    changeFontSize,
  }: {
    connectionId: string
    connected: boolean
    settings: Settings
    register: (handle: TerminalHandle | null) => void
    reportError: (message: string) => void
    changeFontSize: (delta: number) => void
  } = $props()
  let container: HTMLDivElement
  let terminal: Terminal | undefined
  const terminalTheme = (theme: Settings['theme']) =>
    theme === 'light'
      ? {
          background: '#f3f5fb',
          foreground: '#20283e',
          cursor: '#576fb5',
          selectionBackground: '#576fb540',
        }
      : {
          background: '#0d1019',
          foreground: '#edf0f8',
          cursor: '#5ec4af',
          selectionBackground: '#576fb566',
        }
  let resizeTerminal = () => {}
  $effect(() => {
    const { fontSize, scrollback, theme } = settings
    if (terminal) {
      terminal.options.fontSize = fontSize
      terminal.options.scrollback = scrollback
      terminal.options.theme = terminalTheme(theme)
      void tick().then(() => resizeTerminal())
    }
  })
  onMount(() => {
    terminal = new Terminal({
      fontFamily: 'Cascadia Mono, Consolas, monospace',
      fontSize: settings.fontSize,
      scrollback: settings.scrollback,
      cursorBlink: true,
      allowProposedApi: false,
      theme: terminalTheme(settings.theme),
    })
    const fit = new FitAddon()
    terminal.loadAddon(fit)
    terminal.open(container)
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return
      event.preventDefault()
      event.stopImmediatePropagation()
      if (event.deltaY) changeFontSize(event.deltaY < 0 ? 1 : -1)
    }
    container.addEventListener('wheel', wheel, {
      capture: true,
      passive: false,
    })
    const resize = () => {
      if (!container.clientWidth || !container.clientHeight) return
      fit.fit()
      void api
        .resize(connectionId, terminal!.cols, terminal!.rows)
        .catch(() => {})
    }
    resizeTerminal = resize
    terminal.attachCustomKeyEventHandler(
      (event) =>
        !(event.ctrlKey && event.key === 'Tab') &&
        !(
          event.altKey &&
          event.key === 'F4' &&
          !event.ctrlKey &&
          !event.metaKey
        ),
    )
    const input = terminal.onData((data) => {
      if (!connected) return
      void api
        .input(connectionId, data)
        .catch((error) => reportError(String(error)))
    })
    const observer = new ResizeObserver(resize)
    observer.observe(container)
    register({
      write: (data, sequence) =>
        terminal!.write(new Uint8Array(data), () => {
          void api.ack(connectionId, sequence).catch(() => {})
        }),
      fit: resize,
      focus: () => terminal!.focus(),
      clear: () => terminal!.clear(),
    })
    resize()
    return () => {
      register(null)
      observer.disconnect()
      container.removeEventListener('wheel', wheel, true)
      resizeTerminal = () => {}
      input.dispose()
      terminal?.dispose()
      terminal = undefined
    }
  })
</script>

<div class="terminal-frame">
  <div class="terminal-container" bind:this={container}></div>
</div>

<style>
  .terminal-frame {
    width: 100%;
    height: 100%;
    padding: 12px;
    overflow: hidden;
  }
  /* FitAddon reads the parent's computed size, including border-box padding.
     Keep its measured container free of padding so every row stays visible. */
  .terminal-container {
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
</style>
