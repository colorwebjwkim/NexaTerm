<script lang="ts">
  import type { Snippet } from 'svelte'

  let {
    expanded,
    terminal,
    files,
  }: {
    expanded: boolean
    terminal: Snippet
    files: Snippet
  } = $props()
  let ratio = $state(50)
  const id = $props.id()
  let container: HTMLDivElement
  let drag = $state<{ id: number; y: number; ratio: number } | null>(null)

  function setRatio(value: number) {
    ratio = Math.max(15, Math.min(85, value))
  }
  function start(event: PointerEvent) {
    if (event.button !== 0) return
    event.preventDefault()
    const target = event.currentTarget as HTMLDivElement
    target.focus()
    target.setPointerCapture(event.pointerId)
    drag = { id: event.pointerId, y: event.clientY, ratio }
  }
  function move(event: PointerEvent) {
    if (!drag || drag.id !== event.pointerId) return
    const height = container.clientHeight - 8
    if (height > 0)
      setRatio(drag.ratio + ((event.clientY - drag.y) / height) * 100)
  }
  function end(event: PointerEvent) {
    if (drag?.id !== event.pointerId) return
    drag = null
    const target = event.currentTarget as HTMLDivElement
    if (target.hasPointerCapture(event.pointerId))
      target.releasePointerCapture(event.pointerId)
  }
  function key(event: KeyboardEvent) {
    const values: Record<string, number> = {
      ArrowUp: ratio - 5,
      ArrowDown: ratio + 5,
      Home: 15,
      End: 85,
    }
    if (!(event.key in values)) return
    event.preventDefault()
    setRatio(values[event.key])
  }
</script>

<div
  class="session-split"
  class:dragging={Boolean(drag)}
  bind:this={container}
  style:grid-template-rows={expanded
    ? `minmax(0, ${ratio}fr) 8px minmax(0, ${100 - ratio}fr)`
    : 'minmax(0, 1fr)'}
>
  <div class="ssh-area" id={`${id}-ssh`}>{@render terminal()}</div>
  {#if expanded}
    <!-- A focusable separator is the WAI-ARIA window splitter pattern. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="splitter"
      role="separator"
      aria-label="SSH와 SFTP 영역 높이 조절"
      aria-orientation="horizontal"
      aria-valuemin="15"
      aria-valuemax="85"
      aria-valuenow={Math.round(ratio)}
      aria-controls={`${id}-ssh ${id}-sftp`}
      tabindex="0"
      onpointerdown={start}
      onpointermove={move}
      onpointerup={end}
      onpointercancel={end}
      onlostpointercapture={() => (drag = null)}
      onkeydown={key}
      ondblclick={() => setRatio(50)}
      title="드래그로 높이 조절 · 두 번 클릭하면 균등 분할"
    >
      <span></span>
    </div>
    <div class="sftp-area" id={`${id}-sftp`}>{@render files()}</div>
  {/if}
</div>

<style>
  .session-split {
    flex: 1;
    min-height: 0;
    min-width: 0;
    display: grid;
    overflow: hidden;
  }
  .ssh-area,
  .sftp-area {
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }
  .ssh-area {
    display: flex;
  }
  .dragging {
    user-select: none;
    cursor: row-resize;
  }
  .splitter {
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: row-resize;
    touch-action: none;
    background: var(--panel);
    border-block: 1px solid var(--line);
    z-index: 1;
  }
  .splitter span {
    width: 40px;
    height: 2px;
    border-radius: 2px;
    background: var(--muted);
  }
  .splitter:hover,
  .splitter:focus-visible,
  .dragging .splitter {
    background: var(--hover);
    outline: none;
  }
  .splitter:focus-visible span,
  .splitter:hover span {
    background: var(--accent);
  }
</style>
