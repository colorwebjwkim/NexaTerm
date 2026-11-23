<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { fileSelection } from '../fileSelection'
  import {
    files,
    size,
    type FileEntry,
    type Listing,
    type LocalRoot,
  } from '../files'
  let {
    connectionId,
    changed,
    revision = 0,
  }: { connectionId: string; changed: () => void; revision?: number } = $props()
  let root = $state<LocalRoot | null>(null),
    local = $state<Listing>({ path: '', entries: [] }),
    remote = $state<Listing>({ path: '.', entries: [] })
  // Format only the label; file operations retain the granted canonical path.
  let rootLabel = $derived(
    (root?.path ?? '')
      .replace(/^\\\\\?\\UNC\\/i, '\\\\')
      .replace(/^\\\\\?\\(?=[a-z]:\\)/i, ''),
  )
  let localPath = $state(''),
    remotePath = $state('.'),
    localSelected = $state<string[]>([]),
    remoteSelected = $state<string[]>([])
  let busy = $state(false),
    error = $state(''),
    showHidden = $state(false),
    collision = $state('ask')
  let operation = $state<{ side: string; action: string; name: string } | null>(
      null,
    ),
    newName = $state(''),
    mode = $state('644'),
    confirmed = $state(false)
  async function act(task: () => Promise<void>) {
    if (busy) return
    busy = true
    error = ''
    try {
      await task()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
  let lastRevision = -1
  $effect(() => {
    if (revision !== lastRevision) {
      lastRevision = revision
      untrack(() => {
        if (root && !busy) void act(() => refresh(true))
      })
    }
  })
  async function refresh(preserve = false) {
    const localSelection = localSelected,
      remoteSelection = remoteSelected
    const result = await files.remote(connectionId, remotePath || '.')
    remote = result
    remotePath = result.path
    remoteSelected = preserve
      ? remoteSelection.filter((name) =>
          result.entries.some((entry) => entry.name === name),
        )
      : []
    if (root) {
      local = await files.local(root.token, localPath)
      localPath = local.path
      localSelected = preserve
        ? localSelection.filter((name) =>
            local.entries.some((entry) => entry.name === name),
          )
        : []
    }
  }
  async function choose() {
    await act(async () => {
      const selected = await files.chooseRoot()
      if (selected) {
        root = selected
        localPath = ''
        local = await files.local(root.token, '')
        localSelected = []
      }
    })
  }
  async function navigate(side: string, entry?: FileEntry, up = false) {
    await act(async () => {
      if (side === 'local' && root) {
        const target = up
          ? local.path.split('/').slice(0, -1).join('/')
          : entry
            ? [local.path, entry.name].filter(Boolean).join('/')
            : localPath
        local = await files.local(root.token, target, true)
        localPath = local.path
        localSelected = []
      } else if (side === 'remote') {
        const target = up
          ? remote.path.split('/').slice(0, -1).join('/') || '/'
          : entry
            ? `${remote.path.replace(/\/$/, '')}/${entry.name}`
            : remotePath
        remote = await files.remote(connectionId, target)
        remotePath = remote.path
        remoteSelected = []
      }
    })
  }
  async function transfer(direction: string) {
    await act(async () => {
      if (!root) throw new Error('로컬 폴더를 먼저 선택하세요')
      await files.enqueue({
        connectionId,
        direction,
        localRoot: root.token,
        localDir: local.path,
        remoteDir: remote.path,
        names: direction === 'upload' ? localSelected : remoteSelected,
        collision,
      })
      changed()
    })
  }
  function openOperation(side: string, action: string, name = '') {
    operation = { side, action, name }
    newName = action === 'rename' ? name : ''
    mode = '644'
    confirmed = false
    error = ''
  }
  async function applyOperation() {
    await act(async () => {
      if (!operation) return
      if (operation.side === 'local' && !root)
        throw new Error('로컬 폴더를 선택하세요')
      if (operation.action === 'chmod' && !/^[0-7]{3}$/.test(mode))
        throw new Error('권한을 000~777로 입력하세요')
      await files.operation(
        connectionId,
        operation.side,
        root?.token || '',
        operation.side === 'local' ? local.path : remote.path,
        operation.action === 'mkdir' ? newName : operation.name,
        operation.action,
        newName,
        operation.action === 'chmod' ? parseInt(mode, 8) : null,
        confirmed,
      )
      operation = null
      await refresh()
    })
  }
  onMount(() => {
    void act(async () => {
      try {
        const location = await files.openLocal()
        root = location.root
        local = location.listing
        localPath = local.path
      } catch (e) {
        error = String(e)
      }
      await refresh()
    })
  })
</script>

<section class="sftp-pane" aria-label="SFTP 파일 관리자">
  <div class="sftp-heading">
    <strong>SFTP 파일 관리자</strong><label class="inline"
      ><input type="checkbox" bind:checked={showHidden} />숨김 파일</label
    ><label class="inline"
      >동일 이름<select bind:value={collision}
        ><option value="ask">매번 묻기</option><option value="skip"
          >건너뛰기</option
        ><option value="rename">이름 변경</option><option value="overwrite"
          >덮어쓰기</option
        ></select
      ></label
    ><button disabled={busy} onclick={() => act(refresh)}>새로고침</button
    ><button
      disabled={busy}
      onclick={() =>
        act(async () => {
          await files.reset(connectionId)
          await refresh()
        })}>SFTP 재연결</button
    >
  </div>
  <div class="dual-pane">
    {#each ['local', 'remote'] as side (side)}
      {@const listing = side === 'local' ? local : remote}
      {@const selected = side === 'local' ? localSelected : remoteSelected}
      {@const visibleEntries = listing.entries.filter(
        (entry) => showHidden || !entry.name.startsWith('.'),
      )}
      <section
        class="file-pane"
        aria-label={side === 'local' ? '로컬 파일' : '원격 파일'}
      >
        <div class="pane-title">
          <strong>{side === 'local' ? '로컬' : '원격'}</strong
          >{#if side === 'local'}<span
              class="root-label"
              title={side === 'local' ? rootLabel : undefined}
              >{rootLabel || '폴더를 선택하세요'}</span
            ><button disabled={busy} onclick={choose}>폴더 선택</button>{/if}
        </div>
        <form
          class="path-row"
          onsubmit={(e) => {
            e.preventDefault()
            void navigate(side)
          }}
        >
          <button
            type="button"
            aria-label={`${side} 상위 폴더`}
            disabled={busy || (side === 'local' && !root)}
            onclick={() => navigate(side, undefined, true)}>↑</button
          ><input
            aria-label={side === 'local' ? '로컬 상대 경로' : '원격 경로'}
            disabled={busy || (side === 'local' && !root)}
            value={side === 'local' ? localPath : remotePath}
            oninput={(e) => {
              if (side === 'local') localPath = e.currentTarget.value
              else remotePath = e.currentTarget.value
            }}
          /><button disabled={busy || (side === 'local' && !root)}>이동</button>
        </form>
        <div class="file-actions">
          <button
            disabled={busy || (side === 'local' && !root)}
            onclick={() => openOperation(side, 'mkdir')}>새 폴더</button
          ><button
            disabled={busy || selected.length !== 1}
            onclick={() => openOperation(side, 'rename', selected[0])}
            >이름 변경</button
          ><button
            disabled={busy || selected.length !== 1}
            onclick={() => openOperation(side, 'delete', selected[0])}
            >삭제</button
          >{#if side === 'remote'}<button
              disabled={busy || selected.length !== 1}
              onclick={() => openOperation(side, 'chmod', selected[0])}
              >권한</button
            >{/if}<span></span><button
            class="primary"
            disabled={busy || !root || selected.length === 0}
            onclick={() => transfer(side === 'local' ? 'upload' : 'download')}
            >{side === 'local' ? '업로드 →' : '← 다운로드'} ({selected.length})</button
          >
        </div>
        <div
          class="file-table"
          use:fileSelection={{
            names: visibleEntries
              .filter((entry) => !entry.blocked)
              .map((entry) => entry.name),
            selected,
            path: listing.path,
            open: (name) => {
              const entry = listing.entries.find((entry) => entry.name === name)
              if (entry?.directory && !entry.blocked) void navigate(side, entry)
            },
            up: () => {
              void navigate(side, undefined, true)
            },
            change: (names) => {
              if (side === 'local') localSelected = names
              else remoteSelected = names
            },
          }}
        >
          <table
            role="grid"
            tabindex="0"
            aria-label={side === 'local' ? '로컬 파일 목록' : '원격 파일 목록'}
            aria-multiselectable="true"
          >
            <thead
              ><tr
                ><th class="check-col"
                  ><input
                    type="checkbox"
                    aria-label={`${side} 전체 선택`}
                    checked={listing.entries.filter(
                      (e) =>
                        !e.blocked && (showHidden || !e.name.startsWith('.')),
                    ).length > 0 &&
                      listing.entries
                        .filter(
                          (e) =>
                            !e.blocked &&
                            (showHidden || !e.name.startsWith('.')),
                        )
                        .every((e) => selected.includes(e.name))}
                    onchange={(e) => {
                      const names = e.currentTarget.checked
                        ? listing.entries
                            .filter(
                              (e) =>
                                !e.blocked &&
                                (showHidden || !e.name.startsWith('.')),
                            )
                            .map((e) => e.name)
                        : []
                      if (side === 'local') localSelected = names
                      else remoteSelected = names
                    }}
                  /></th
                ><th>이름</th><th>크기</th><th>수정일</th></tr
              ></thead
            ><tbody
              >{#each visibleEntries as entry (entry.name)}<tr
                  class:blocked={entry.blocked}
                  class:selected={selected.includes(entry.name)}
                  aria-selected={selected.includes(entry.name)}
                  data-file-name={entry.name}
                  ondblclick={(event) => {
                    if (
                      entry.directory &&
                      !entry.blocked &&
                      !(event.target instanceof HTMLInputElement)
                    )
                      void navigate(side, entry)
                  }}
                  ><td
                    ><input
                      type="checkbox"
                      tabindex="-1"
                      aria-label={`${side} ${entry.name} 선택`}
                      disabled={entry.blocked}
                      checked={selected.includes(entry.name)}
                    /></td
                  ><td
                    ><button
                      class="file-name"
                      tabindex="-1"
                      disabled={entry.blocked}
                      onclick={(event) => {
                        if (
                          event.detail === 0 &&
                          entry.directory &&
                          !entry.blocked
                        )
                          void navigate(side, entry)
                      }}
                      title={entry.blocked
                        ? '링크/특수 파일 또는 지원하지 않는 파일명'
                        : entry.name}
                      >{entry.blocked ? '⊘' : entry.directory ? '▸' : '·'}
                      {entry.name}</button
                    ></td
                  ><td>{entry.directory ? '폴더' : size(entry.size)}</td><td
                    >{entry.modified
                      ? new Date(entry.modified * 1000).toLocaleString('ko-KR')
                      : '—'}</td
                  ></tr
                >{/each}</tbody
            >
          </table>
          {#if !visibleEntries.length}<p class="empty">
              {busy
                ? '불러오는 중…'
                : side === 'local' && !root
                  ? '로컬 폴더를 선택하세요.'
                  : listing.entries.length
                    ? '표시할 파일이 없습니다. 숨김 파일 표시를 확인하세요.'
                    : '파일이 없습니다.'}
            </p>{/if}
        </div>
      </section>
    {/each}
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if operation}<div
      class="file-confirm"
      role="dialog"
      aria-modal="true"
      aria-label="파일 작업 확인"
    >
      <h3>
        {operation.side === 'local' ? '로컬' : '원격'}
        {operation.action === 'mkdir'
          ? '폴더 생성'
          : operation.action === 'rename'
            ? '이름 변경'
            : operation.action === 'chmod'
              ? '권한 변경'
              : '삭제'}
      </h3>
      <p>{operation.name}</p>
      {#if operation.action === 'mkdir' || operation.action === 'rename'}<label
          >이름<input bind:value={newName} /></label
        >{:else if operation.action === 'chmod'}<label
          >권한 (000~777)<input bind:value={mode} maxlength="3" /></label
        >{:else}<label class="inline"
          ><input type="checkbox" bind:checked={confirmed} />이 파일/빈 폴더를
          삭제합니다.</label
        >{/if}
      <div class="confirm-buttons">
        <button disabled={busy} onclick={() => (operation = null)}>취소</button
        ><button
          class="primary"
          disabled={busy || (operation.action === 'delete' && !confirmed)}
          onclick={applyOperation}>확인 후 적용</button
        >
      </div>
    </div>{/if}
</section>

<style>
  .sftp-pane {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    overflow: auto;
    overscroll-behavior: contain;
    border-top: 1px solid var(--line);
    background: var(--panel);
    font-size: 12px;
  }
  .sftp-heading,
  .pane-title,
  .path-row,
  .file-actions {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 8px 10px;
    flex-shrink: 0;
  }
  .sftp-heading {
    flex-wrap: wrap;
    border-bottom: 1px solid var(--line);
  }
  .sftp-heading strong {
    margin-right: auto;
  }
  .sftp-pane button {
    font-size: 11px;
    padding: 5px 8px;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }
  .inline input {
    width: 13px;
  }
  .inline select {
    padding: 4px;
    width: 100px;
    font-size: 11px;
  }
  .dual-pane {
    display: grid;
    grid-template-columns: 1fr 1fr;
    flex: 1;
    min-height: 0;
  }
  .file-pane {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .file-pane:first-child {
    border-right: 1px solid var(--line);
  }
  .root-label {
    font-size: 10px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .path-row {
    padding-top: 0;
  }
  .path-row input {
    padding: 5px;
    font-size: 11px;
    width: auto;
    flex: 1;
  }
  .path-row button {
    flex-shrink: 0;
    white-space: nowrap;
  }
  .file-actions {
    border-top: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .file-actions span {
    flex: 1;
  }
  .file-table {
    position: relative;
    overflow: auto;
    overscroll-behavior: contain;
    flex: 1;
    min-height: 0;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    text-align: left;
    font-size: 11px;
    white-space: nowrap;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--bg);
    padding: 6px;
    color: var(--muted);
    font-weight: 400;
  }
  td {
    border-bottom: 1px solid var(--line);
    padding: 4px 6px;
  }
  td input,
  th input {
    width: 13px;
  }
  .file-name {
    border: 0;
    background: transparent;
    padding: 1px !important;
    text-align: left;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  tr.selected td {
    background: color-mix(in srgb, var(--accent) 22%, var(--panel));
  }
  .file-table:focus-within :global(tr.focused) {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  table:focus {
    outline: none;
  }
  tbody tr:not(.blocked) {
    cursor: default;
  }
  .file-table :global(.file-selection-box) {
    position: absolute;
    pointer-events: none;
    z-index: 2;
    border: 1px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
  }
  .blocked {
    opacity: 0.5;
  }
  .empty {
    color: var(--muted);
    padding: 10px;
    overflow-wrap: anywhere;
    white-space: normal;
  }
  .error {
    margin: 5px 10px;
  }
  .file-confirm {
    position: absolute;
    inset: 15px max(15%, 30px);
    z-index: 5;
    background: var(--panel);
    border: 1px solid var(--accent);
    box-shadow: 0 10px 60px #000b;
    border-radius: 8px;
    padding: 18px;
    overflow: auto;
  }
  .file-confirm h3 {
    margin: 0 0 12px;
  }
  .file-confirm label {
    display: block;
  }
  .file-confirm input {
    margin-top: 6px;
  }
  .file-confirm .inline {
    display: flex;
  }
  .confirm-buttons {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    margin-top: 15px;
  }
</style>
