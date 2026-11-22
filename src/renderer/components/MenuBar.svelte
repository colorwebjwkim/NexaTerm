<script lang="ts" module>
  export type MenuItem = {
    label: string
    shortcut?: string
    disabled?: boolean
    checked?: boolean
    separator?: boolean
    action: () => void
  }
  export type AppMenu = { label: string; items: MenuItem[] }
</script>

<script lang="ts">
  import { tick } from 'svelte'
  let { menus, disabled = false }: { menus: AppMenu[]; disabled?: boolean } =
    $props()
  let open = $state(-1)
  let current = $state(0)
  let root: HTMLElement

  function trigger(index: number) {
    return root.querySelector<HTMLButtonElement>(`[data-trigger="${index}"]`)
  }
  function dismiss(restoreFocus = false) {
    const previous = open
    open = -1
    if (restoreFocus && previous >= 0) trigger(previous)?.focus()
  }
  async function show(index: number, last = false) {
    if (disabled) return
    open = current = index
    await tick()
    const items = root.querySelectorAll<HTMLButtonElement>(
      '[role="menu"] button:not(:disabled)',
    )
    ;(last ? items[items.length - 1] : items[0])?.focus()
  }
  function outside(event: Event) {
    if (event.target instanceof Node && !root.contains(event.target)) dismiss()
  }
  function menuKey(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      dismiss(true)
    } else if (event.key === 'Tab') {
      dismiss(true)
    } else if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      event.preventDefault()
      const next =
        (current + (event.key === 'ArrowRight' ? 1 : -1) + menus.length) %
        menus.length
      current = next
      if (open >= 0) void show(next)
      else trigger(next)?.focus()
    } else if (
      open >= 0 &&
      ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)
    ) {
      event.preventDefault()
      const items = Array.from(
        root.querySelectorAll<HTMLButtonElement>(
          '[role="menu"] button:not(:disabled)',
        ),
      )
      const index = items.indexOf(document.activeElement as HTMLButtonElement)
      const next =
        event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? items.length - 1
            : (index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) %
              items.length
      items[next]?.focus()
    }
  }
  $effect(() => {
    if (disabled) open = -1
  })
</script>

<svelte:window onpointerdown={outside} onfocusin={outside} />
<nav class="app-menu" aria-label="메인 메뉴">
  <div
    class="menu-bar"
    role="menubar"
    tabindex="-1"
    aria-label="애플리케이션"
    bind:this={root}
    onkeydown={menuKey}
  >
    {#each menus as menu, index (menu.label)}
      <div class="menu-group">
        <button
          class="menu-trigger"
          class:expanded={open === index}
          role="menuitem"
          data-trigger={index}
          aria-haspopup="menu"
          aria-expanded={open === index}
          tabindex={current === index ? 0 : -1}
          {disabled}
          onfocus={() => (current = index)}
          onclick={() => (open === index ? dismiss() : void show(index))}
          onkeydown={(event) => {
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault()
              event.stopPropagation()
              void show(index, event.key === 'ArrowUp')
            }
          }}>{menu.label}</button
        >
        {#if open === index}
          <div class="menu-popup" role="menu" aria-label={menu.label}>
            {#each menu.items as item (item.label)}
              {#if item.separator}<div
                  class="menu-separator"
                  role="separator"
                ></div>{/if}
              <button
                class="menu-entry"
                role={item.checked === undefined
                  ? 'menuitem'
                  : 'menuitemcheckbox'}
                aria-checked={item.checked}
                disabled={item.disabled}
                tabindex="-1"
                onclick={() => {
                  dismiss(true)
                  item.action()
                }}
                ><span class="menu-check" aria-hidden="true"
                  >{item.checked ? '✓' : ''}</span
                ><span>{item.label}</span><span class="menu-shortcut"
                  >{item.shortcut || ''}</span
                ></button
              >
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>
</nav>

<style>
  .app-menu {
    display: flex;
  }
  .menu-bar {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .menu-group {
    position: relative;
  }
  .menu-trigger {
    border: 0;
    border-radius: 4px;
    background: transparent;
    padding: 5px 12px;
  }
  .menu-trigger.expanded {
    background: var(--hover);
  }
  .menu-popup {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 30;
    min-width: 255px;
    padding: 5px;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: var(--panel);
    box-shadow: 0 8px 24px #0004;
  }
  .menu-entry {
    display: flex;
    align-items: center;
    width: 100%;
    gap: 8px;
    border: 0;
    border-radius: 4px;
    padding: 8px;
    text-align: left;
    background: transparent;
    white-space: nowrap;
  }
  .menu-entry:focus-visible {
    outline-offset: -2px;
    background: var(--hover);
  }
  .menu-check {
    width: 14px;
    color: var(--accent);
  }
  .menu-shortcut {
    margin-left: auto;
    padding-left: 20px;
    font-size: 12px;
    color: var(--muted);
  }
  .menu-separator {
    margin: 5px 8px;
    border-top: 1px solid var(--line);
  }
</style>
