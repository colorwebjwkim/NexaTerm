<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import { version as appVersion } from '../../package.json'
  import SftpPane from './components/SftpPane.svelte'
  import SessionSplit from './components/SessionSplit.svelte'
  import MenuBar, { type AppMenu } from './components/MenuBar.svelte'
  import TransferQueue from './components/TransferQueue.svelte'
  import { files, activeTransfer, type Transfer } from './files'
  import { validateProfile } from './profileValidation'
  import {
    api,
    type AppView,
    type Profile,
    type Preview,
    type SessionEvent,
  } from './api'
  import TerminalView, {
    type TerminalHandle,
  } from './components/TerminalView.svelte'
  type Tab = {
    id: string
    attempt?: string
    authenticationRejected?: boolean
    connectionId: string
    profileId: string
    title: string
    state: string
    message: string
    filesOpen: boolean
    connectionType: 'ssh' | 'sftp'
  }
  type HostKey = Extract<SessionEvent, { kind: 'hostKey' }> & { tabId: string }
  let view = $state<AppView>({
    profiles: [],
    settings: {
      theme: 'dark',
      fontSize: 14,
      scrollback: 5000,
      confirmExit: true,
    },
  })
  let ready = $state(false),
    error = $state(''),
    busy = $state(false),
    query = $state('')
  let serverListOpen = $state(true)
  let tabDrag = $state<{
    id: string
    pointerId: number
    x: number
    y: number
    started: boolean
  } | null>(null)
  let tabDrop = $state<{ id: string; after: boolean } | null>(null)
  let suppressTabClick = false
  let tabs = $state<Tab[]>([]),
    activeId = $state(''),
    hostKeys = $state<HostKey[]>([]),
    changedConfirmed = $state(false)
  let dialog = $state<
    | 'profile'
    | 'backup'
    | 'restore'
    | 'settings'
    | 'connect'
    | 'help'
    | 'about'
    | null
  >(null)
  let editing = $state<Profile>(blankProfile()),
    password = $state(''),
    saveSecret = $state(false),
    tagText = $state('')
  let backupPassword = $state(''),
    backupConfirm = $state(''),
    includeSecrets = $state(false),
    preview = $state<Preview | null>(null)
  let restoreMode = $state('merge'),
    replaceConfirmed = $state(false),
    connectProfile = $state<Profile | null>(null),
    saveConnectSecret = $state(false)
  let reconnectTabId = $state<string | null>(null)
  let profileSubmitted = $state(false)
  let profileErrors = $derived(
    profileSubmitted
      ? validateProfile(
          editing,
          tagText,
          password,
          saveSecret,
          view.profiles.find((p) => p.id === editing.id),
        )
      : {},
  )
  $effect(() => {
    if (dialog || hostKeys.length || closingTab || busy) return
    const tab = tabs.find(
      (t) => t.authenticationRejected && t.state === 'error',
    )
    if (!tab) return
    tab.authenticationRejected = false
    const profile = view.profiles.find((p) => p.id === tab.profileId)
    if (!profile || profile.authMethod !== 'password') return
    activeId = tab.id
    reconnectTabId = tab.id
    connectProfile = profile
    password = ''
    saveConnectSecret = false
    error =
      '비밀번호 인증이 거부되었습니다. 계정과 비밀번호를 확인하고 다시 입력하세요.'
    dialog = 'connect'
  })
  let settingsDraft = $state({
    theme: 'dark' as 'dark' | 'light',
    fontSize: 14,
    scrollback: 5000,
    confirmExit: true,
  })
  const handles = new SvelteMap<string, TerminalHandle>(),
    queues = new SvelteMap<string, { data: number[]; sequence: number }[]>()
  let filtered = $derived(
    view.profiles
      .filter((p) =>
        `${p.name} ${p.host} ${p.username} ${p.group} ${p.tags.join(' ')}`
          .toLowerCase()
          .includes(query.toLowerCase()),
      )
      .sort(
        (a, b) =>
          Number(b.favorite) - Number(a.favorite) ||
          a.group.localeCompare(b.group) ||
          a.name.localeCompare(b.name),
      ),
  )
  let active = $derived(tabs.find((t) => t.id === activeId)),
    hostKey = $derived(hostKeys[0])
  let jobs = $state<Transfer[]>([]),
    closingTab = $state<Tab | null>(null)
  let closingBusy = $state(false)
  let polling = false
  let fontSave = Promise.resolve()
  function openSettings() {
    settingsDraft = { ...view.settings }
    dialog = 'settings'
    error = ''
  }
  async function focusSearch() {
    serverListOpen = true
    await tick()
    document.querySelector<HTMLInputElement>('.search')?.focus()
  }
  let terminalAvailable = $derived(
    Boolean(
      active && active.connectionType === 'ssh' && active.state === 'connected',
    ),
  )
  let activeProfile = $derived(
    view.profiles.find((profile) => profile.id === active?.profileId),
  )
  let menus = $derived<AppMenu[]>([
    {
      label: '파일',
      items: [
        { label: '새 서버 프로필…', shortcut: 'Ctrl+T', action: () => edit() },
        {
          label: '현재 탭 닫기',
          shortcut: 'Ctrl+W',
          disabled: !active,
          action: () => {
            if (active) void closeTab(active)
          },
        },
        {
          label: '설정 백업…',
          separator: true,
          action: () => openBackup('backup'),
        },
        {
          label: '설정 복원…',
          disabled: tabs.some(
            (tab) => !['error', 'disconnected'].includes(tab.state),
          ),
          action: () => openBackup('restore'),
        },
      ],
    },
    {
      label: '연결',
      items: [
        { label: '서버 찾기', action: focusSearch },
        {
          label: '같은 서버에 새 연결',
          disabled: !activeProfile,
          action: () => {
            if (activeProfile) void requestConnect(activeProfile)
          },
        },
        {
          label: '현재 서버 프로필 편집…',
          disabled: !activeProfile,
          action: () => {
            if (activeProfile) edit(activeProfile)
          },
        },
        {
          label: '연결 종료',
          separator: true,
          disabled: !active || ['error', 'disconnected'].includes(active.state),
          action: () => {
            if (active) void closeTab(active)
          },
        },
      ],
    },
    {
      label: '보기',
      items: [
        {
          label: '시작 화면',
          disabled: tabs.length > 0,
          action: () => {
            if (!tabs.length) activeId = ''
          },
        },
        {
          label: '다음 탭',
          shortcut: 'Ctrl+Tab',
          disabled: !tabs.length,
          action: () => {
            const index = tabs.findIndex((tab) => tab.id === activeId)
            void select(tabs[(index + 1) % tabs.length].id)
          },
        },
        {
          label: '이전 탭',
          shortcut: 'Ctrl+Shift+Tab',
          disabled: !tabs.length,
          action: () => {
            const index = tabs.findIndex((tab) => tab.id === activeId)
            void select(tabs[(index - 1 + tabs.length) % tabs.length].id)
          },
        },
        {
          label: 'SFTP 파일 패널',
          shortcut: 'F9',
          checked: Boolean(
            active?.filesOpen || active?.connectionType === 'sftp',
          ),
          disabled: !terminalAvailable,
          separator: true,
          action: () => {
            if (active) active.filesOpen = !active.filesOpen
          },
        },
        {
          label: '터미널 글자 확대',
          disabled: !terminalAvailable || view.settings.fontSize >= 28,
          action: () => changeFontSize(1),
        },
        {
          label: '터미널 글자 축소',
          disabled: !terminalAvailable || view.settings.fontSize <= 10,
          action: () => changeFontSize(-1),
        },
        {
          label: '터미널 화면 지우기',
          separator: true,
          disabled: !terminalAvailable,
          action: () => {
            if (active) handles.get(active.id)?.clear()
          },
        },
      ],
    },
    { label: '도구', items: [{ label: '환경 설정…', action: openSettings }] },
    {
      label: '도움말',
      items: [
        {
          label: '키보드 단축키',
          shortcut: 'F1',
          action: () => (dialog = 'help'),
        },
        {
          label: 'NexaTerm 정보',
          separator: true,
          action: () => (dialog = 'about'),
        },
      ],
    },
  ])
  function changeFontSize(delta: number) {
    if (!ready || busy || dialog || hostKeys.length) return
    const fontSize = Math.max(10, Math.min(28, view.settings.fontSize + delta))
    if (fontSize === view.settings.fontSize) return
    view.settings.fontSize = fontSize
    const settings = $state.snapshot(view.settings)
    fontSave = fontSave
      .then(async () => {
        await api.settings(settings)
      })
      .catch(fail)
  }
  async function pollJobs() {
    if (polling) return
    polling = true
    try {
      jobs = await files.jobs()
    } catch {
      /* App shutdown ends IPC. */
    } finally {
      polling = false
    }
  }
  function blankProfile(): Profile {
    return {
      id: crypto.randomUUID(),
      name: '',
      host: '',
      port: 22,
      username: '',
      group: '',
      tags: [],
      favorite: false,
      authMethod: 'password',
      keyPath: '',
      secretRef: null,
      connectionType: 'ssh',
      hostKeyAlgorithm: 'ssh-rsa',
    }
  }
  function fail(value: unknown) {
    error = String(value)
  }
  $effect(() => {
    if (dialog || hostKeys.length || closingTab) {
      const selector = hostKeys.length
        ? '.host-overlay .modal'
        : '.overlay .modal'
      void tick().then(() => {
        const modal = document.querySelector<HTMLElement>(selector)
        const target =
          modal?.querySelector<HTMLElement>('input, select') ||
          modal?.querySelector<HTMLElement>('button')
        target?.focus()
      })
    }
  })
  async function action(task: () => Promise<void>) {
    if (busy) return
    busy = true
    error = ''
    try {
      await task()
    } catch (e) {
      fail(e)
    } finally {
      busy = false
    }
  }
  onMount(() => {
    const timer = setInterval(() => void pollJobs(), 500)
    void api
      .state()
      .then((result) => {
        view = result
        ready = true
      })
      .catch(fail)
    return () => clearInterval(timer)
  })
  function edit(profile?: Profile) {
    profileSubmitted = false
    error = ''
    password = ''
    editing = profile
      ? structuredClone($state.snapshot(profile))
      : blankProfile()
    tagText = editing.tags.join(', ')
    saveSecret = Boolean(editing.secretRef)
    dialog = 'profile'
  }
  async function saveProfile() {
    profileSubmitted = true
    if (Object.keys(profileErrors).length) {
      error = '표시된 입력 항목을 확인하세요.'
      await tick()
      document.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus()
      return
    }
    await action(async () => {
      editing.tags = tagText
        .split(',')
        .map((x) => x.trim())
        .filter(Boolean)
      view = await api.save($state.snapshot(editing), password, saveSecret)
      password = ''
      dialog = null
    })
  }
  async function remove(profile: Profile) {
    if (
      !window.confirm(
        `“${profile.name}” 프로필과 저장된 비밀번호를 삭제할까요?`,
      )
    )
      return
    await action(async () => {
      view = await api.remove(profile.id)
    })
  }
  async function requestConnect(profile: Profile, tabId: string | null = null) {
    reconnectTabId = tabId
    if (profile.secretRef) await openConnection(profile, '', tabId)
    else {
      connectProfile = profile
      password = ''
      saveConnectSecret = false
      error = ''
      dialog = 'connect'
    }
  }
  async function submitConnection() {
    if (!connectProfile) return
    const profile = $state.snapshot(connectProfile)
    const supplied = password
    if (saveConnectSecret && supplied) {
      view = await api.save(profile, supplied, true)
    }
    await openConnection(profile, supplied, reconnectTabId)
  }
  async function reconnect(tab: Tab) {
    if (!['error', 'disconnected'].includes(tab.state) || busy) return
    const profile = view.profiles.find((p) => p.id === tab.profileId)
    if (!profile) return fail('프로필이 삭제되어 재접속할 수 없습니다')
    await requestConnect(profile, tab.id)
  }
  async function openConnection(
    profile: Profile,
    supplied: string,
    tabId: string | null = null,
  ) {
    const id = tabId || crypto.randomUUID(),
      count = tabs.filter((t) => t.profileId === profile.id).length + 1
    const existing = tabs.find((tab) => tab.id === id)
    if (tabId && !existing) return
    if (existing) {
      if (!['error', 'disconnected'].includes(existing.state)) return
      existing.attempt = ''
      existing.authenticationRejected = false
      existing.state = 'connecting'
      existing.message = '재접속 준비 중'
      const oldConnectionId = existing.connectionId
      existing.connectionId = ''
      existing.filesOpen = false
      queues.delete(id)
      hostKeys = hostKeys.filter((key) => key.tabId !== id)
      if (oldConnectionId) await api.disconnect(oldConnectionId).catch(() => {})
      await tick()
    } else
      tabs.push({
        id,
        connectionId: '',
        profileId: profile.id,
        title: `${profile.name}${profile.connectionType === 'sftp' ? ' · SFTP' : ''} #${count}`,
        state: 'connecting',
        message: '연결 준비 중',
        filesOpen: false,
        connectionType: profile.connectionType,
      })
    activeId = id
    password = ''
    dialog = null
    const attempt = crypto.randomUUID()
    const current = tabs.find((tab) => tab.id === id)!
    current.attempt = attempt
    try {
      const cid = await api.connect(profile.id, supplied, (event) => {
        const tab = tabs.find((t) => t.id === id)
        if (!tab || tab.attempt !== attempt) return
        if (event.kind === 'state') {
          tab.state = event.state
          tab.message = event.message
          if (['error', 'disconnected'].includes(event.state))
            hostKeys = hostKeys.filter((k) => k.tabId !== id)
          if (event.state === 'connected')
            void tick().then(() => {
              handles.get(id)?.fit()
              handles.get(id)?.focus()
            })
        } else if (event.kind === 'authenticationRejected') {
          tab.authenticationRejected = true
        } else if (event.kind === 'data') {
          const handle = handles.get(id)
          if (handle) handle.write(event.data, event.sequence)
          else {
            const queue = queues.get(id) || []
            queue.push(event)
            queues.set(id, queue)
          }
        } else {
          hostKeys.push({ ...event, tabId: id })
          changedConfirmed = false
        }
      })
      const tab = tabs.find((t) => t.id === id)
      if (tab) tab.connectionId = cid
      else await api.disconnect(cid)
    } catch (e) {
      const tab = tabs.find((t) => t.id === id)
      if (tab) {
        tab.state = 'error'
        tab.message = String(e)
      }
      fail(e)
    }
  }
  function register(id: string, handle: TerminalHandle | null) {
    if (!handle) {
      handles.delete(id)
      return
    }
    handles.set(id, handle)
    for (const item of queues.get(id) || [])
      handle.write(item.data, item.sequence)
    queues.delete(id)
  }
  async function select(id: string) {
    activeId = id
    await tick()
    handles.get(id)?.fit()
    handles.get(id)?.focus()
  }
  function startTabDrag(event: PointerEvent, id: string) {
    if (event.button !== 0 || !event.isPrimary) return
    suppressTabClick = false
    const button = event.currentTarget as HTMLButtonElement
    button.setPointerCapture(event.pointerId)
    tabDrag = {
      id,
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      started: false,
    }
  }
  function moveTabDrag(event: PointerEvent) {
    if (!tabDrag || tabDrag.pointerId !== event.pointerId) return
    if (!tabDrag.started) {
      if (Math.hypot(event.clientX - tabDrag.x, event.clientY - tabDrag.y) < 6)
        return
      tabDrag.started = true
    }
    const bar = (event.currentTarget as HTMLElement).closest('.tabbar')!
    const bounds = bar.getBoundingClientRect()
    tabDrop = null
    if (
      event.clientY < bounds.top ||
      event.clientY > bounds.bottom ||
      event.clientX < bounds.left ||
      event.clientX > bounds.right
    )
      return
    if (event.clientX < bounds.left + 32) bar.scrollLeft -= 24
    if (event.clientX > bounds.right - 32) bar.scrollLeft += 24
    const others = Array.from(
      bar.querySelectorAll<HTMLElement>('[data-tab-id]'),
    ).filter((element) => element.dataset.tabId !== tabDrag?.id)
    for (const element of others) {
      const rect = element.getBoundingClientRect()
      if (event.clientX < rect.left + rect.width / 2) {
        tabDrop = { id: element.dataset.tabId!, after: false }
        return
      }
    }
    const last = others.at(-1)
    if (last) tabDrop = { id: last.dataset.tabId!, after: true }
  }
  function endTabDrag(event?: PointerEvent, commit = false) {
    if (!tabDrag || (event && tabDrag.pointerId !== event.pointerId)) return
    if (tabDrag.started) {
      // Pointer capture delivers the following click to the dragged button.
      suppressTabClick = true
      if (commit && tabDrop) {
        const moving = tabs.find((tab) => tab.id === tabDrag?.id)
        const remaining = tabs.filter((tab) => tab.id !== tabDrag?.id)
        const target = remaining.findIndex((tab) => tab.id === tabDrop?.id)
        if (moving && target !== -1) {
          remaining.splice(target + Number(tabDrop.after), 0, moving)
          tabs = remaining
        }
      }
    }
    tabDrag = null
    tabDrop = null
  }
  async function closeTab(tab: Tab) {
    await pollJobs()
    if (
      jobs.some(
        (job) => job.connectionId === tab.connectionId && activeTransfer(job),
      )
    ) {
      closingTab = tab
      return
    }
    if (
      !['error', 'disconnected'].includes(tab.state) &&
      !window.confirm(`“${tab.title}” 연결을 종료할까요?`)
    )
      return
    await finishClose(tab)
  }
  async function finishClose(tab: Tab, policy?: string) {
    if (closingBusy) return
    closingBusy = true
    try {
      if (tab.connectionId) await api.disconnect(tab.connectionId, policy)
    } catch (e) {
      closingBusy = false
      fail(e)
      return
    }
    tabs = tabs.filter((t) => t.id !== tab.id)
    hostKeys = hostKeys.filter((k) => k.tabId !== tab.id)
    queues.delete(tab.id)
    if (activeId === tab.id) activeId = tabs.at(-1)?.id || ''
    closingTab = null
    closingBusy = false
  }
  async function trust(approve: boolean) {
    if (!hostKey) return
    const token = hostKey.token
    try {
      await api.trust(token, approve, changedConfirmed)
      hostKeys = hostKeys.filter((k) => k.token !== token)
      changedConfirmed = false
    } catch (e) {
      fail(e)
    }
  }
  function openBackup(type: 'backup' | 'restore') {
    error = ''
    backupPassword = ''
    backupConfirm = ''
    includeSecrets = false
    preview = null
    restoreMode = 'merge'
    replaceConfirmed = false
    dialog = type
  }
  async function exportBackup() {
    await action(async () => {
      if (backupPassword !== backupConfirm)
        throw new Error('백업 암호가 일치하지 않습니다')
      const file = await api.backup(backupPassword, includeSecrets)
      if (file) {
        dialog = null
        backupPassword = ''
        backupConfirm = ''
      }
    })
  }
  async function previewBackup() {
    await action(async () => {
      preview = await api.preview(backupPassword)
      backupPassword = ''
    })
  }
  async function restoreBackup() {
    await action(async () => {
      if (!preview) return
      view = await api.restore(preview.token, restoreMode, replaceConfirmed)
      dialog = null
      preview = null
    })
  }
  async function closeDialog() {
    if (busy) return
    password = ''
    backupPassword = ''
    backupConfirm = ''
    preview = null
    dialog = null
    error = ''
    await api.cancelRestore().catch(() => {})
  }
  onMount(() => {
    window.addEventListener('keydown', shortcut, true)
    return () => window.removeEventListener('keydown', shortcut, true)
  })
  function shortcut(e: KeyboardEvent) {
    if (closingTab) {
      if (e.key === 'Escape' && !closingBusy) closingTab = null
      return
    }
    if (
      e.key === 'F9' &&
      active &&
      active.state === 'connected' &&
      active.connectionType !== 'sftp' &&
      !dialog &&
      !hostKey
    ) {
      e.preventDefault()
      active.filesOpen = !active.filesOpen
      return
    }
    if (dialog || hostKey) {
      if (e.key === 'Escape') {
        e.preventDefault()
        if (hostKey) void trust(false)
        else void closeDialog()
      }
      if (e.key === 'Tab') {
        const root = document.querySelector(
          hostKey ? '.host-overlay .modal' : '.overlay .modal',
        )
        const controls = Array.from(
          root?.querySelectorAll<HTMLElement>(
            'button:not(:disabled),input:not(:disabled),select:not(:disabled)',
          ) || [],
        )
        const first = controls[0],
          last = controls.at(-1)
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault()
          last?.focus()
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault()
          first?.focus()
        }
      }
      return
    }
    if (
      e.key === 'Enter' &&
      !e.repeat &&
      !e.isComposing &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.shiftKey &&
      active &&
      ['error', 'disconnected'].includes(active.state) &&
      !(
        e.target instanceof Element &&
        e.target.closest(
          'button,input,select,textarea:not(.xterm-helper-textarea),[contenteditable=true],[role=menuitem]',
        )
      )
    ) {
      e.preventDefault()
      e.stopImmediatePropagation()
      void reconnect(active)
      return
    }
    if (e.key === 'F1') {
      e.preventDefault()
      dialog = 'help'
    }
    if (e.ctrlKey && e.key.toLowerCase() === 't') {
      e.preventDefault()
      edit()
    }
    if (e.ctrlKey && e.key.toLowerCase() === 'w' && active) {
      e.preventDefault()
      void closeTab(active)
    }
    if (e.ctrlKey && e.key === 'Tab' && tabs.length) {
      e.preventDefault()
      e.stopImmediatePropagation()
      const index = tabs.findIndex((t) => t.id === activeId)
      void select(
        tabs[(index + (e.shiftKey ? -1 : 1) + tabs.length) % tabs.length].id,
      )
    }
  }
</script>

<div
  class="workspace"
  class:sidebar-collapsed={!serverListOpen}
  data-theme={view.settings.theme}
>
  <header>
    <MenuBar
      {menus}
      disabled={!ready || busy || Boolean(dialog || hostKey || closingTab)}
    />
  </header>
  <aside id="server-sidebar" hidden={!serverListOpen}>
    <div class="sidebar-heading">
      <h2>서버 목록</h2>
      <div class="sidebar-buttons">
        <button
          class="icon-button"
          aria-label="새 프로필"
          disabled={!ready}
          onclick={() => edit()}>＋</button
        >
        <button
          class="sidebar-toggle"
          aria-label="서버 목록 접기"
          aria-expanded="true"
          aria-controls="server-sidebar"
          title="서버 목록 접기"
          onclick={() => (serverListOpen = false)}
          ><svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            aria-hidden="true"><path d="m14 6-6 6 6 6" /></svg
          ></button
        >
      </div>
    </div>
    <div id="server-browser" class="server-browser">
      <input
        class="search"
        aria-label="서버 검색"
        placeholder="이름, 호스트, 그룹 검색"
        bind:value={query}
      />
      <div class="server-list">
        {#each filtered as profile (profile.id)}<article class="server-card">
            <button
              class="server-connect"
              title={profile.name}
              aria-label={`${profile.name} 연결`}
              onclick={() => requestConnect(profile)}
              ><span class="server-avatar" aria-hidden="true">
                {#if profile.favorite}★{:else}<svg
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.7"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    {#if profile.connectionType === 'sftp'}<path
                        d="M3 7V5a1 1 0 0 1 1-1h5l2 3h9a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1Z"
                      />{:else}<rect
                        x="3"
                        y="4"
                        width="18"
                        height="16"
                        rx="2"
                      /><path d="m7 9 3 3-3 3m6 0h4" />{/if}
                  </svg>{/if}
              </span><span class="server-name">{profile.name}</span></button
            >
            <div class="server-actions">
              <button
                aria-label="편집"
                title={`${profile.name} 편집`}
                onclick={() => edit(profile)}
                ><svg
                  width="15"
                  height="15"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.7"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"
                  ><path d="m16 3 5 5-12 12-6 1 1-6Z" /><path
                    d="m14 5 5 5"
                  /></svg
                ></button
              ><button
                class="danger-text"
                aria-label="삭제"
                title={`${profile.name} 삭제`}
                onclick={() => remove(profile)}
                ><svg
                  width="15"
                  height="15"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.7"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"
                  ><path
                    d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7m4-7v7"
                  /></svg
                ></button
              >
            </div>
          </article>{/each}
        {#if !filtered.length}<div class="sidebar-empty">
            {query ? '검색 결과가 없습니다.' : '등록된 서버가 없습니다.'}
          </div>{/if}
      </div>
    </div>
  </aside>
  <main>
    <div class="tabbar" class:tab-dragging={tabDrag?.started}>
      {#if !serverListOpen}<button
          class="sidebar-toggle"
          aria-label="서버 목록 펼치기"
          aria-expanded="false"
          aria-controls="server-sidebar"
          title="서버 목록 펼치기"
          onclick={() => (serverListOpen = true)}
          ><svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            aria-hidden="true"><path d="m10 6 6 6-6 6" /></svg
          ></button
        >{/if}
      {#if !tabs.length}<button class="active" onclick={() => (activeId = '')}
          >시작</button
        >{/if}{#each tabs as tab (tab.id)}<div
          class="tab"
          class:active={activeId === tab.id}
          class:drag-source={tabDrag?.started && tabDrag.id === tab.id}
          class:drop-before={tabDrop?.id === tab.id && !tabDrop.after}
          class:drop-after={tabDrop?.id === tab.id && tabDrop.after}
          data-tab-id={tab.id}
        >
          <button
            class="tab-select"
            title={`${tab.title} · 드래그하여 탭 순서 변경`}
            onpointerdown={(event) => startTabDrag(event, tab.id)}
            onpointermove={moveTabDrag}
            onpointerup={(event) => {
              moveTabDrag(event)
              endTabDrag(event, true)
            }}
            onpointercancel={(event) => endTabDrag(event)}
            onlostpointercapture={(event) => endTabDrag(event)}
            onkeydown={(event) => {
              if (event.key === 'Escape') endTabDrag()
            }}
            onclick={() => {
              if (!suppressTabClick) void select(tab.id)
              suppressTabClick = false
            }}
            ><span
              class="tab-dot"
              class:connected={tab.state === 'connected'}
              class:failed={tab.state === 'error'}
            ></span>{tab.title}</button
          ><button
            class="tab-close"
            aria-label={`${tab.title} 닫기`}
            onclick={() => closeTab(tab)}
            ><span class="tab-close-icon">×</span></button
          >
        </div>{/each}
    </div>
    <div class="main-body" class:session-active={Boolean(activeId)}>
      {#if !tabs.length}<section class="welcome">
          <h1>서버 연결</h1>
          {#if view.profiles.length}<p>왼쪽 목록에서 서버를 선택하세요.</p>{/if}
          <button class="primary large" disabled={!ready} onclick={() => edit()}
            >＋ 서버 프로필 만들기</button
          >
        </section>{/if}
      {#each tabs as tab (tab.id)}<div
          class="terminal-pane"
          class:hidden={activeId !== tab.id}
        >
          <div class="session-toolbar">
            <span>{tab.message}</span><span class="spacer"
            ></span>{#if tab.connectionType !== 'sftp'}<button
                disabled={tab.state !== 'connected'}
                onclick={() => (tab.filesOpen = !tab.filesOpen)}
                >{tab.filesOpen ? '파일 닫기' : '파일 열기 (SFTP)'}</button
              ><button onclick={() => handles.get(tab.id)?.clear()}
                >화면 지우기</button
              >{/if}{#if !['error', 'disconnected'].includes(tab.state)}<button
                class="danger-text"
                onclick={() => closeTab(tab)}>연결 종료</button
              >{:else}<button onclick={() => reconnect(tab)}
                >재접속 (Enter)</button
              >{/if}
          </div>
          {#if tab.connectionType === 'sftp'}
            {#if tab.connectionId && tab.state === 'connected'}<div
                class="sftp-only"
              >
                <SftpPane
                  connectionId={tab.connectionId}
                  revision={jobs.filter(
                    (job) =>
                      job.connectionId === tab.connectionId &&
                      ['completed', 'failed', 'cancelled'].includes(job.status),
                  ).length}
                  changed={() => void pollJobs()}
                />
              </div>{/if}
          {:else}<SessionSplit
              expanded={tab.filesOpen && tab.state === 'connected'}
            >
              {#snippet terminal()}<div class="terminal-space">
                  {#if tab.connectionId}<TerminalView
                      connectionId={tab.connectionId}
                      connected={tab.state === 'connected'}
                      settings={view.settings}
                      register={(handle) => register(tab.id, handle)}
                      reportError={fail}
                      {changeFontSize}
                    />{/if}
                </div>{/snippet}
              {#snippet files()}<SftpPane
                  connectionId={tab.connectionId}
                  revision={jobs.filter(
                    (job) =>
                      job.connectionId === tab.connectionId &&
                      ['completed', 'failed', 'cancelled'].includes(job.status),
                  ).length}
                  changed={() => void pollJobs()}
                />{/snippet}
            </SessionSplit>{/if}
          {#if tab.state === 'disconnected'}<div class="session-error">
              연결 종료 · Enter 키로 재접속할 수 있습니다.
            </div>{/if}
          {#if tab.state === 'error'}<div class="session-error" role="alert">
              {tab.message} · Enter 키 또는 재접속 버튼으로 다시 시도하세요.
            </div>{/if}
        </div>{/each}
    </div>
    <TransferQueue {jobs} changed={() => void pollJobs()} reportError={fail} />
  </main>
  <footer>
    <span class="spacer"></span><span
      >활성 연결 {tabs.filter((t) => t.state === 'connected').length} · 프로필 {view
        .profiles.length}</span
    >
    {#if active?.connectionType !== 'sftp'}<div
        class="font-controls"
        role="group"
        aria-label="콘솔 글꼴 크기"
      >
        <button
          aria-label="콘솔 글꼴 축소"
          title="글꼴 축소 (Ctrl + 휠 아래)"
          disabled={!ready || busy || view.settings.fontSize <= 10}
          onclick={() => changeFontSize(-1)}
          ><svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            aria-hidden="true"
            ><circle cx="10" cy="10" r="7" /><path d="m15 15 6 6M6 10h8" /></svg
          ></button
        >
        <span aria-live="polite" aria-atomic="true"
          >{view.settings.fontSize}px</span
        >
        <button
          aria-label="콘솔 글꼴 확대"
          title="글꼴 확대 (Ctrl + 휠 위)"
          disabled={!ready || busy || view.settings.fontSize >= 28}
          onclick={() => changeFontSize(1)}
          ><svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            aria-hidden="true"
            ><circle cx="10" cy="10" r="7" /><path
              d="m15 15 6 6M6 10h8M10 6v8"
            /></svg
          ></button
        >
      </div>{/if}
  </footer>
  {#if error && !dialog && !hostKey}<div class="toast" role="alert">
      {error}<button aria-label="알림 닫기" onclick={() => (error = '')}
        >×</button
      >
    </div>{/if}
  {#if closingTab}<div class="overlay">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-label="전송 중 탭 닫기"
        tabindex="-1"
      >
        <h2>진행 중인 전송이 있습니다</h2>
        <p>{closingTab.title} 탭을 닫을 때 전송을 어떻게 처리할까요?</p>
        <div class="modal-actions">
          <button disabled={closingBusy} onclick={() => (closingTab = null)}
            >닫기 취소</button
          ><button
            disabled={closingBusy}
            onclick={() => closingTab && finishClose(closingTab, 'cancel')}
            >전송 취소 후 닫기</button
          ><button
            class="primary"
            disabled={closingBusy}
            onclick={() => closingTab && finishClose(closingTab, 'keep')}
            >전송 유지하고 탭 닫기</button
          >
        </div>
      </div>
    </div>{/if}
  {#if dialog}<div class="overlay">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="dialog-title"
        tabindex="-1"
      >
        <div class="modal-title">
          <h2 id="dialog-title">
            {dialog === 'profile'
              ? '서버 프로필'
              : dialog === 'backup'
                ? '암호화 백업'
                : dialog === 'restore'
                  ? '설정 복원'
                  : dialog === 'settings'
                    ? '작업공간 설정'
                    : dialog === 'help'
                      ? '키보드 단축키'
                      : dialog === 'about'
                        ? 'NexaTerm 정보'
                        : connectProfile?.connectionType === 'sftp'
                          ? 'SFTP 접속'
                          : 'SSH 접속'}
          </h2>
          <button aria-label="닫기" disabled={busy} onclick={closeDialog}
            >×</button
          >
        </div>
        {#if dialog === 'profile'}<form
            class="profile-form"
            novalidate
            onsubmit={(e) => {
              e.preventDefault()
              void saveProfile()
            }}
          >
            <label
              >프로필 이름<input
                required
                maxlength="160"
                aria-label="프로필 이름"
                aria-invalid={profileErrors.name ? true : undefined}
                aria-describedby={profileErrors.name
                  ? 'profile-name-error'
                  : undefined}
                bind:value={editing.name}
                placeholder="예: 운영 서버"
              />{#if profileErrors.name}<span
                  id="profile-name-error"
                  class="field-error">{profileErrors.name}</span
                >{/if}</label
            >
            <div class="form-row">
              <label class="grow"
                >호스트<input
                  required
                  maxlength="253"
                  aria-label="호스트"
                  aria-invalid={profileErrors.host ? true : undefined}
                  aria-describedby={profileErrors.host
                    ? 'profile-host-error'
                    : undefined}
                  bind:value={editing.host}
                  placeholder="192.168.0.10 또는 server.example.com"
                />{#if profileErrors.host}<span
                    id="profile-host-error"
                    class="field-error">{profileErrors.host}</span
                  >{/if}</label
              ><label class="port"
                >포트<input
                  type="number"
                  min="1"
                  max="65535"
                  required
                  aria-label="포트"
                  aria-invalid={profileErrors.port ? true : undefined}
                  aria-describedby={profileErrors.port
                    ? 'profile-port-error'
                    : undefined}
                  bind:value={editing.port}
                />{#if profileErrors.port}<span
                    id="profile-port-error"
                    class="field-error">{profileErrors.port}</span
                  >{/if}</label
              >
            </div>
            <label
              >사용자명<input
                required
                maxlength="128"
                aria-label="사용자명"
                aria-invalid={profileErrors.username ? true : undefined}
                aria-describedby={profileErrors.username
                  ? 'profile-username-error'
                  : undefined}
                bind:value={editing.username}
                autocomplete="off"
              />{#if profileErrors.username}<span
                  id="profile-username-error"
                  class="field-error">{profileErrors.username}</span
                >{/if}</label
            >
            <div class="form-row">
              <label class="grow"
                >그룹<input
                  maxlength="160"
                  aria-label="그룹"
                  aria-invalid={profileErrors.group ? true : undefined}
                  aria-describedby={profileErrors.group
                    ? 'profile-group-error'
                    : undefined}
                  bind:value={editing.group}
                  placeholder="개발 / 운영"
                />{#if profileErrors.group}<span
                    id="profile-group-error"
                    class="field-error">{profileErrors.group}</span
                  >{/if}</label
              ><label class="grow"
                >태그<input
                  aria-label="태그"
                  aria-invalid={profileErrors.tags ? true : undefined}
                  aria-describedby={profileErrors.tags
                    ? 'profile-tags-error'
                    : undefined}
                  bind:value={tagText}
                  placeholder="쉼표로 구분"
                />{#if profileErrors.tags}<span
                    id="profile-tags-error"
                    class="field-error">{profileErrors.tags}</span
                  >{/if}</label
              >
            </div>
            <label
              >접속 방식<select bind:value={editing.connectionType}
                ><option value="ssh">SSH</option><option value="sftp"
                  >SFTP 전용</option
                ></select
              ></label
            ><label
              >호스트키 알고리즘<select bind:value={editing.hostKeyAlgorithm}>
                <option value="ssh-rsa">ssh-rsa 우선 (기본 · 호환 모드)</option>
                <option value="auto">자동 (라이브러리 기본 순서)</option>
                <option value="rsa-sha2-512">rsa-sha2-512</option>
                <option value="rsa-sha2-256">rsa-sha2-256</option>
                <option value="ssh-ed25519">ssh-ed25519</option>
                <option value="ecdsa-sha2-nistp256">ECDSA nistp256</option>
                <option value="ecdsa-sha2-nistp384">ECDSA nistp384</option>
                <option value="ecdsa-sha2-nistp521">ECDSA nistp521</option>
              </select></label
            >
            <p class="form-help">
              ssh-rsa는 구형 SHA-1 서명을 사용합니다. 우선 협상이 불가능하면
              다른 지원 알고리즘을 사용하며, 개별 알고리즘 선택 시 해당 방식만
              허용합니다.
            </p>
            <label
              >인증 방식<select bind:value={editing.authMethod}
                ><option value="password">비밀번호</option><option
                  value="privateKey">개인키 파일</option
                ></select
              ></label
            >
            {#if editing.authMethod === 'privateKey'}<label
                >개인키 경로
                <div class="form-row">
                  <input
                    class="grow"
                    required
                    aria-label="개인키 경로"
                    aria-invalid={profileErrors.keyPath ? true : undefined}
                    aria-describedby={profileErrors.keyPath
                      ? 'profile-keyPath-error'
                      : undefined}
                    bind:value={editing.keyPath}
                  /><button
                    type="button"
                    onclick={async () => {
                      try {
                        const path = await api.keyFile()
                        if (path) editing.keyPath = path
                      } catch (e) {
                        fail(e)
                      }
                    }}>찾기</button
                  >
                </div>
                {#if profileErrors.keyPath}<span
                    id="profile-keyPath-error"
                    class="field-error">{profileErrors.keyPath}</span
                  >{/if}</label
              >{/if}
            <label
              >{editing.authMethod === 'privateKey'
                ? '개인키 passphrase'
                : '비밀번호'}<input
                type="password"
                aria-label={editing.authMethod === 'privateKey'
                  ? '개인키 passphrase'
                  : '비밀번호'}
                aria-invalid={profileErrors.password ? true : undefined}
                aria-describedby={profileErrors.password
                  ? 'profile-password-error'
                  : undefined}
                bind:value={password}
                autocomplete="new-password"
                placeholder={editing.secretRef
                  ? '비워두면 저장된 값 유지'
                  : '비밀번호 입력'}
              />{#if profileErrors.password}<span
                  id="profile-password-error"
                  class="field-error">{profileErrors.password}</span
                >{/if}</label
            ><label class="checkbox"
              ><input type="checkbox" bind:checked={saveSecret} />이 PC에
              비밀번호 저장</label
            ><label class="checkbox"
              ><input
                type="checkbox"
                bind:checked={editing.favorite}
              />즐겨찾기</label
            >
            {#if editing.secretRef}<p class="form-help">
                저장을 해제하면 기존 비밀번호가 삭제됩니다.
              </p>{/if}
            {#if error}<p class="error" role="alert">{error}</p>{/if}
            <div class="modal-actions">
              <button type="button" disabled={busy} onclick={closeDialog}
                >취소</button
              ><button class="primary" disabled={busy}
                >{busy ? '저장 중…' : '프로필 저장'}</button
              >
            </div>
          </form>
        {:else if dialog === 'connect'}<form
            onsubmit={(e) => {
              e.preventDefault()
              void action(submitConnection)
            }}
          >
            <p>{connectProfile?.username}@{connectProfile?.host}</p>
            <label
              >{connectProfile?.authMethod === 'privateKey'
                ? '개인키 passphrase (없으면 비워 두세요)'
                : '비밀번호'}<input
                type="password"
                required={connectProfile?.authMethod !== 'privateKey'}
                bind:value={password}
                autocomplete="current-password"
              /></label
            >
            <label class="checkbox"
              ><input type="checkbox" bind:checked={saveConnectSecret} />이 PC에
              비밀번호 저장</label
            >
            {#if error}<p class="error" role="alert">{error}</p>{/if}
            <div class="modal-actions">
              <button type="button" onclick={closeDialog}>취소</button><button
                class="primary"
                disabled={busy}>연결</button
              >
            </div>
          </form>
        {:else if dialog === 'backup'}<form
            onsubmit={(e) => {
              e.preventDefault()
              void exportBackup()
            }}
          >
            <label class="checkbox"
              ><input type="checkbox" bind:checked={includeSecrets} />저장된
              비밀번호 / passphrase 포함</label
            ><label
              >별도 백업 암호<input
                type="password"
                minlength="12"
                required
                bind:value={backupPassword}
                autocomplete="new-password"
                placeholder="12자 이상"
              /></label
            ><label
              >백업 암호 확인<input
                type="password"
                minlength="12"
                required
                bind:value={backupConfirm}
                autocomplete="new-password"
              /></label
            >
            <p class="form-help">
              암호를 잃어버리면 복구할 수 없습니다. 개인키 파일과 호스트키 신뢰
              정보는 포함하지 않습니다.
            </p>
            {#if error}<p class="error" role="alert">{error}</p>{/if}
            <div class="modal-actions">
              <button type="button" disabled={busy} onclick={closeDialog}
                >취소</button
              ><button class="primary" disabled={busy}
                >{busy ? '암호화 중…' : '파일로 백업'}</button
              >
            </div>
          </form>
        {:else if dialog === 'restore'}
          {#if !preview}<form
              onsubmit={(e) => {
                e.preventDefault()
                void previewBackup()
              }}
            >
              <label
                >백업 암호<input
                  type="password"
                  minlength="12"
                  required
                  bind:value={backupPassword}
                  autocomplete="off"
                /></label
              >{#if error}<p class="error" role="alert">{error}</p>{/if}
              <div class="modal-actions">
                <button type="button" disabled={busy} onclick={closeDialog}
                  >취소</button
                ><button class="primary" disabled={busy}
                  >{busy ? '검증 중…' : '파일 선택 및 미리보기'}</button
                >
              </div>
            </form>
          {:else}<p>
              프로필 {preview.profiles.length}개 · 저장된 비밀번호 {preview.passwordCount}개
            </p>
            <div class="preview-list">
              {#each preview.profiles as name, index (index)}<div>
                  {name}
                </div>{/each}
            </div>
            <label
              >복원 방식<select bind:value={restoreMode}
                ><option value="merge"
                  >병합 · 기존 프로필 유지, ID 충돌 시 사본 추가</option
                ><option value="replace"
                  >전체 교체 · 기존 프로필과 비밀번호 삭제</option
                ></select
              ></label
            >{#if restoreMode === 'replace'}<label class="checkbox warning"
                ><input type="checkbox" bind:checked={replaceConfirmed} />기존
                프로필·비밀번호·앱 설정을 교체하는 데 동의합니다.</label
              >{/if}
            <p class="form-help">
              병합은 현재 앱 설정을 유지합니다. 호스트키 신뢰는 가져오지 않으며,
              키 파일은 직접 다시 지정해야 합니다. 연결은 자동 실행하지
              않습니다.
            </p>
            {#if error}<p class="error" role="alert">{error}</p>{/if}
            <div class="modal-actions">
              <button disabled={busy} onclick={closeDialog}>취소</button><button
                class="primary"
                disabled={busy ||
                  (restoreMode === 'replace' && !replaceConfirmed)}
                onclick={restoreBackup}>확인 후 복원</button
              >
            </div>{/if}
        {:else if dialog === 'settings'}<form
            onsubmit={(e) => {
              e.preventDefault()
              void action(async () => {
                await fontSave
                view = await api.settings(settingsDraft)
                dialog = null
              })
            }}
          >
            <label
              >테마<select bind:value={settingsDraft.theme}
                ><option value="dark">어두운 테마</option><option value="light"
                  >밝은 테마</option
                ></select
              ></label
            ><label
              >터미널 글자 크기<input
                type="number"
                min="10"
                max="28"
                bind:value={settingsDraft.fontSize}
                required
              /></label
            ><label
              >스크롤 버퍼 줄 수<input
                type="number"
                min="100"
                max="20000"
                bind:value={settingsDraft.scrollback}
                required
              /></label
            ><label class="checkbox"
              ><input
                type="checkbox"
                bind:checked={settingsDraft.confirmExit}
              />활성 연결이 있을 때 앱 종료 확인창 표시</label
            >
            <p class="form-help">
              끄면 앱 종료 시 연결과 진행 중인 전송을 확인 없이 종료합니다.
            </p>
            {#if error}<p class="error" role="alert">{error}</p>{/if}
            <div class="modal-actions">
              <button type="button" onclick={closeDialog}>취소</button><button
                class="primary"
                disabled={busy}>설정 저장</button
              >
            </div>
          </form>
        {:else if dialog === 'help'}
          <dl class="shortcut-list">
            <div>
              <dt>새 서버 프로필</dt>
              <dd>Ctrl+T</dd>
            </div>
            <div>
              <dt>현재 탭 닫기</dt>
              <dd>Ctrl+W</dd>
            </div>
            <div>
              <dt>다음 / 이전 탭</dt>
              <dd>Ctrl+Tab / Ctrl+Shift+Tab</dd>
            </div>
            <div>
              <dt>SFTP 파일 패널</dt>
              <dd>F9</dd>
            </div>
            <div>
              <dt>터미널 글자 크기</dt>
              <dd>Ctrl+마우스 휠</dd>
            </div>
            <div>
              <dt>단축키 도움말</dt>
              <dd>F1</dd>
            </div>
          </dl>
        {:else if dialog === 'about'}
          <p><strong>NexaTerm</strong></p>
          <p>버전 {appVersion}</p>
          <p>SSH 터미널 및 SFTP 클라이언트</p>
        {/if}
      </div>
    </div>{/if}
  {#if hostKey}<div class="overlay host-overlay">
      <div
        class="modal"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="host-title"
        tabindex="-1"
      >
        <h2 id="host-title">
          {hostKey.previous
            ? '서버 호스트키가 변경되었습니다'
            : '처음 연결하는 서버입니다'}
        </h2>
        <p>{hostKey.host}</p>
        <div class="info-field">알고리즘<code>{hostKey.algorithm}</code></div>
        <div class="info-field">
          현재 지문<code>{hostKey.fingerprint}</code>
        </div>
        {#if hostKey.previous}<div class="info-field">
            이전 지문<code>{hostKey.previous}</code>
          </div>
          <p class="warning">
            서버 재설치 또는 중간자 공격일 수 있습니다. 관리자에게 새 지문을
            확인하기 전에는 연결하지 마세요.
          </p>
          <label class="checkbox"
            ><input type="checkbox" bind:checked={changedConfirmed} />별도
            경로로 변경된 지문을 확인했습니다.</label
          >{:else}<p class="form-help">
            관리자에게 지문을 확인한 뒤 승인하세요. 승인한 지문은 다음 연결부터
            비교합니다.
          </p>{/if}{#if error}<p class="error" role="alert">{error}</p>{/if}
        <div class="modal-actions">
          <button onclick={() => trust(false)}>연결 거부</button><button
            class="primary"
            disabled={Boolean(hostKey.previous) && !changedConfirmed}
            onclick={() => trust(true)}
            >{hostKey.previous ? '변경된 키 재승인' : '신뢰하고 연결'}</button
          >
        </div>
      </div>
    </div>{/if}
</div>
