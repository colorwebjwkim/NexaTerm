interface Options {
  names: string[]
  selected: string[]
  path: string
  change: (names: string[]) => void
  open: (name: string) => void
  up: () => void
}

let nextGridId = 0

// Each panel owns its anchor and pointer gesture, independent of the other panel.
export function fileSelection(node: HTMLElement, initial: Options) {
  let options = initial
  let anchor: string | undefined
  let focused: string | undefined
  let search = ''
  let searchTime = 0
  const grid = node.querySelector<HTMLTableElement>('table')!
  const id = `sftp-grid-${++nextGridId}`
  function focusRow(name: string | undefined, scroll = false) {
    focused = name
    let active: HTMLElement | undefined
    node
      .querySelectorAll<HTMLElement>('tr[data-file-name]')
      .forEach((row, index) => {
        row.id = `${id}-${index}`
        const current = row.dataset.fileName === name
        row.classList.toggle('focused', current)
        if (current) active = row
      })
    if (active) {
      grid.setAttribute('aria-activedescendant', active.id)
      if (scroll) {
        const bounds = node.getBoundingClientRect()
        const rect = active.getBoundingClientRect()
        const header =
          node.querySelector('thead')?.getBoundingClientRect().height ?? 0
        if (rect.top < bounds.top + header)
          node.scrollTop += rect.top - bounds.top - header
        else if (rect.bottom > bounds.top + node.clientHeight)
          node.scrollTop += rect.bottom - bounds.top - node.clientHeight
      }
    } else grid.removeAttribute('aria-activedescendant')
  }
  function focusGrid() {
    grid.focus({ preventScroll: true })
  }
  function focusIn(event: FocusEvent) {
    if (event.target === grid && !focused)
      focusRow(
        options.selected.find((name) => options.names.includes(name)) ??
          options.names[0],
      )
  }
  function keydown(event: KeyboardEvent) {
    if (event.defaultPrevented || event.isComposing || gesture) return
    const target = event.target as Element
    // The header checkbox and controls outside the rows keep native keyboard behavior.
    if (target !== grid && !rowAt(target)) return
    const ctrl = event.ctrlKey || event.metaKey
    const navigation = [
      'ArrowUp',
      'ArrowDown',
      'Home',
      'End',
      'PageUp',
      'PageDown',
    ]
    const consume = () => {
      event.preventDefault()
      event.stopPropagation()
    }
    if (
      (event.altKey && event.key === 'ArrowUp') ||
      (!ctrl && !event.altKey && event.key === 'Backspace')
    ) {
      consume()
      if (!event.repeat) options.up()
      return
    }
    if (event.altKey) return
    if (ctrl && event.key.toLowerCase() === 'a') {
      consume()
      options.change([...options.names])
      focusRow(focused ?? options.names[0])
      return
    }
    if (event.key === 'Escape') {
      consume()
      options.change([])
      anchor = focused
      search = ''
      return
    }
    if (!options.names.length) return
    if (navigation.includes(event.key)) {
      consume()
      const current = focused ? options.names.indexOf(focused) : -1
      const rowHeight =
        node.querySelector('tbody tr')?.getBoundingClientRect().height || 25
      const headerHeight =
        node.querySelector('thead')?.getBoundingClientRect().height ?? 0
      const page = Math.max(
        1,
        Math.floor((node.clientHeight - headerHeight) / rowHeight) - 1,
      )
      let index: number
      if (event.key === 'Home') index = 0
      else if (event.key === 'End') index = options.names.length - 1
      else {
        const step = event.key.startsWith('Page') ? page : 1
        const backwards = event.key === 'ArrowUp' || event.key === 'PageUp'
        index =
          current < 0
            ? backwards
              ? options.names.length - 1
              : 0
            : current + (backwards ? -step : step)
      }
      const name =
        options.names[Math.max(0, Math.min(index, options.names.length - 1))]
      if (event.shiftKey && !anchor) anchor = focused ?? name
      if (!ctrl || event.shiftKey) select(name, event.shiftKey, ctrl)
      else if (!anchor) anchor = focused ?? name
      focusRow(name, true)
      focusGrid()
      search = ''
      return
    }
    const name = focused ?? options.names[0]
    if (event.key === ' ') {
      consume()
      if (!event.repeat) select(name, event.shiftKey, ctrl)
      focusRow(name, true)
      focusGrid()
    } else if (event.key === 'Enter' && !ctrl && !event.shiftKey) {
      consume()
      if (!event.repeat) options.open(name)
    } else if (!ctrl && event.key.length === 1) {
      consume()
      const now = Date.now()
      const letter = event.key.toLocaleLowerCase()
      search = now - searchTime > 1000 ? letter : search + letter
      searchTime = now
      const repeated = [...search].every((character) => character === letter)
      const prefix = repeated ? letter : search
      const start = options.names.indexOf(name) + (repeated ? 1 : 0)
      const names = [
        ...options.names.slice(start),
        ...options.names.slice(0, start),
      ]
      const match = names.find((item) =>
        item.toLocaleLowerCase().startsWith(prefix),
      )
      if (match) {
        select(match, false, false)
        focusRow(match, true)
        focusGrid()
      }
    }
  }
  let gesture: {
    id: number
    x: number
    y: number
    clientX: number
    clientY: number
    base: string[]
    additive: boolean
    toggle: boolean
    dragging: boolean
  } | null = null
  let frame = 0
  let suppressClick = false
  const box = document.createElement('div')
  box.className = 'file-selection-box'
  box.hidden = true
  node.append(box)
  const rowAt = (target: EventTarget | null) =>
    target instanceof Element
      ? target.closest<HTMLElement>('tr[data-file-name]')
      : null
  function select(name: string, shift: boolean, toggle: boolean) {
    if (!options.names.includes(name)) return
    const start = anchor ? options.names.indexOf(anchor) : -1
    if (shift && start >= 0) {
      const end = options.names.indexOf(name)
      const range = options.names.slice(
        Math.min(start, end),
        Math.max(start, end) + 1,
      )
      options.change(
        toggle ? [...new Set([...options.selected, ...range])] : range,
      )
    } else {
      options.change(
        toggle
          ? options.selected.includes(name)
            ? options.selected.filter((item) => item !== name)
            : [...options.selected, name]
          : [name],
      )
      anchor = name
    }
  }
  function click(event: MouseEvent) {
    if (suppressClick) {
      suppressClick = false
      event.preventDefault()
      event.stopPropagation()
      return
    }
    const row = rowAt(event.target)
    if (!row) return
    const checkbox = event.target instanceof HTMLInputElement
    select(
      row.dataset.fileName!,
      event.shiftKey,
      checkbox || event.ctrlKey || event.metaKey,
    )
    if (options.names.includes(row.dataset.fileName!)) {
      focusRow(row.dataset.fileName)
      if (!checkbox) focusGrid()
    }
  }
  function point(event: { clientX: number; clientY: number }) {
    const rect = node.getBoundingClientRect()
    return {
      x: event.clientX - rect.left + node.scrollLeft,
      y: event.clientY - rect.top + node.scrollTop,
    }
  }
  function down(event: PointerEvent) {
    if (event.button !== 0 || !event.isPrimary) return
    suppressClick = false
    const target = event.target as Element
    const rect = node.getBoundingClientRect()
    // Leave the header, checkboxes and native scrollbars to their own controls.
    if (
      target.closest('thead, input') ||
      event.clientX >= rect.left + node.clientWidth ||
      event.clientY >= rect.top + node.clientHeight
    )
      return
    const row = rowAt(target)
    if (row && !options.names.includes(row.dataset.fileName!)) return
    suppressClick = false
    const start = point(event)
    gesture = {
      id: event.pointerId,
      ...start,
      clientX: event.clientX,
      clientY: event.clientY,
      base: [...options.selected],
      additive: event.ctrlKey || event.metaKey || event.shiftKey,
      toggle: event.ctrlKey || event.metaKey,
      dragging: false,
    }
  }
  function paint() {
    if (!gesture?.dragging) return
    const current = point(gesture)
    const left = Math.min(gesture.x, current.x)
    const top = Math.min(gesture.y, current.y)
    const right = Math.max(gesture.x, current.x)
    const bottom = Math.max(gesture.y, current.y)
    Object.assign(box.style, {
      left: `${left}px`,
      top: `${top}px`,
      width: `${right - left}px`,
      height: `${bottom - top}px`,
    })
    box.hidden = false
    const bounds = node.getBoundingClientRect()
    const hits = Array.from(
      node.querySelectorAll<HTMLElement>('tr[data-file-name]'),
    )
      .filter((row) => {
        const rect = row.getBoundingClientRect()
        return (
          options.names.includes(row.dataset.fileName!) &&
          rect.right - bounds.left + node.scrollLeft > left &&
          rect.left - bounds.left + node.scrollLeft < right &&
          rect.bottom - bounds.top + node.scrollTop > top &&
          rect.top - bounds.top + node.scrollTop < bottom
        )
      })
      .map((row) => row.dataset.fileName!)
    const base = gesture.base
    options.change(
      gesture.toggle
        ? [
            ...base.filter((name) => !hits.includes(name)),
            ...hits.filter((name) => !base.includes(name)),
          ]
        : gesture.additive
          ? [...new Set([...base, ...hits])]
          : hits,
    )
  }
  function scroll() {
    if (!gesture?.dragging) return
    const rect = node.getBoundingClientRect()
    const header =
      node.querySelector('thead')?.getBoundingClientRect().height ?? 0
    if (gesture.clientY > rect.bottom - 24) node.scrollTop += 12
    else if (gesture.clientY < rect.top + header + 24) node.scrollTop -= 12
    paint()
    frame = requestAnimationFrame(scroll)
  }
  function move(event: PointerEvent) {
    if (!gesture || gesture.id !== event.pointerId) return
    gesture.clientX = event.clientX
    gesture.clientY = event.clientY
    const current = point(event)
    if (
      !gesture.dragging &&
      Math.hypot(current.x - gesture.x, current.y - gesture.y) < 5
    )
      return
    if (!gesture.dragging) {
      gesture.dragging = true
      node.setPointerCapture(event.pointerId)
      frame = requestAnimationFrame(scroll)
    }
    event.preventDefault()
    paint()
  }
  function end(event?: PointerEvent) {
    if (event && gesture?.id !== event.pointerId) return
    if (gesture) {
      suppressClick = gesture.dragging
      if (gesture.dragging) {
        anchor = options.selected[0]
        focusRow(anchor)
        focusGrid()
        // Capture is released below after the gesture has been cleared.
      } else if (
        event?.type === 'pointerup' &&
        !rowAt(event.target) &&
        !gesture.additive
      ) {
        options.change([])
        anchor = undefined
        focusGrid()
      }
    }
    const pointerId = gesture?.id
    gesture = null
    if (pointerId !== undefined && node.hasPointerCapture(pointerId))
      node.releasePointerCapture(pointerId)
    cancelAnimationFrame(frame)
    box.hidden = true
  }
  const blur = () => end()
  node.addEventListener('lostpointercapture', end)
  node.addEventListener('click', click, true)
  node.addEventListener('pointerdown', down)
  node.addEventListener('keydown', keydown, true)
  node.addEventListener('focusin', focusIn)
  window.addEventListener('pointermove', move)
  window.addEventListener('pointerup', end)
  window.addEventListener('pointercancel', end)
  window.addEventListener('blur', blur)
  return {
    update(next: Options) {
      const pathChanged = next.path !== options.path
      if (
        next.path !== options.path ||
        next.names.join('\0') !== options.names.join('\0')
      ) {
        end()
        anchor = undefined
      }
      options = next
      if (pathChanged || (focused && !options.names.includes(focused))) {
        focusRow(undefined)
        search = ''
        if (node.contains(document.activeElement)) {
          focusGrid()
          focusRow(options.names[0])
        }
      } else focusRow(focused)
    },
    destroy() {
      end()
      box.remove()
      node.removeEventListener('click', click, true)
      node.removeEventListener('pointerdown', down)
      node.removeEventListener('keydown', keydown, true)
      node.removeEventListener('focusin', focusIn)
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', end)
      window.removeEventListener('pointercancel', end)
      window.removeEventListener('blur', blur)
      node.removeEventListener('lostpointercapture', end)
    },
  }
}
