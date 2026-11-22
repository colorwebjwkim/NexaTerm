import { expect } from '@playwright/test'
import fs from 'node:fs/promises'
import { createReadStream } from 'node:fs'
import { createHash } from 'node:crypto'
import { join } from 'node:path'
const delay = (milliseconds) =>
  new Promise((resolve) => setTimeout(resolve, milliseconds))
async function hash(path) {
  const digest = createHash('sha256')
  for await (const data of createReadStream(path)) digest.update(data)
  return digest.digest('hex')
}

export async function exerciseSftp(
  page,
  { localRoot, remoteRoot, terminal, behavior, clients },
) {
  const invoke = (command, args = {}) =>
    page.evaluate(
      ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
      { command, args },
    )
  const queue = await invoke('transfer_queue'),
    roots = await invoke('local_roots')
  const connectionId = queue[0].connectionId,
    localRootToken = roots[0].token
  const spec = {
    connectionId,
    direction: 'upload',
    localRoot: localRootToken,
    localDir: '',
    remoteDir: '/',
    names: [],
    collision: 'ask',
  }
  async function enqueue(overrides) {
    return invoke('enqueue_transfer', { spec: { ...spec, ...overrides } })
  }
  async function wait(id, status = 'completed', timeout = 60000) {
    const start = Date.now()
    while (Date.now() - start < timeout) {
      const job = (await invoke('transfer_queue')).find((j) => j.id === id)
      if (job?.status === status) return job
      if (job && ['failed', 'cancelled', 'completed'].includes(job.status))
        throw new Error(`Expected ${status}, got ${job.status}: ${job.message}`)
      await delay(100)
    }
    throw new Error(`SFTP job timeout (${status})`)
  }
  const conflict = await enqueue({ names: ['upload-한글.txt'] })
  await wait(conflict, 'waiting')
  await invoke('resolve_transfer_conflict', { id: conflict, decision: 'skip' })
  expect((await wait(conflict)).skipped).toBe(1)
  await fs.writeFile(
    join(localRoot, 'upload-한글.txt'),
    'changed replacement fixture',
  )
  const overwrite = await enqueue({ names: ['upload-한글.txt'] })
  await wait(overwrite, 'waiting')
  await invoke('resolve_transfer_conflict', {
    id: overwrite,
    decision: 'overwrite',
  })
  await wait(overwrite)
  expect(await fs.readFile(join(remoteRoot, 'upload-한글.txt'), 'utf8')).toBe(
    'changed replacement fixture',
  )
  const rename = await enqueue({
    names: ['upload-한글.txt'],
    collision: 'rename',
  })
  await wait(rename)
  expect(
    await fs.readFile(join(remoteRoot, 'upload-한글.txt (1)'), 'utf8'),
  ).toBe('changed replacement fixture')
  const cancelled = await enqueue({ names: ['upload-한글.txt'] })
  await wait(cancelled, 'waiting')
  await invoke('cancel_transfer', { id: cancelled })
  await wait(cancelled, 'cancelled')
  const retry = await invoke('retry_transfer', { id: cancelled })
  await wait(retry, 'waiting')
  await invoke('resolve_transfer_conflict', { id: retry, decision: 'skip' })
  await wait(retry)
  await expect(terminal.locator('.session-toolbar')).toContainText('SSH 연결됨')
  for (let directory = 0; directory < 3; directory++) {
    const folder = join(localRoot, 'tree', `group-${directory}`)
    await fs.mkdir(folder, { recursive: true })
    await Promise.all(
      Array.from({ length: 100 }, (_, index) =>
        fs.writeFile(
          join(folder, `file-${index}.txt`),
          `NexaTerm ${directory}-${index} 한글 ${'x'.repeat(1024)}`,
        ),
      ),
    )
  }
  const tree = await enqueue({ names: ['tree'], collision: 'skip' })
  expect((await wait(tree, 'completed', 120000)).files).toBe(300)
  await fs.mkdir(join(localRoot, 'received'))
  const downloadTree = await enqueue({
    direction: 'download',
    localDir: 'received',
    names: ['tree'],
    collision: 'skip',
  })
  expect((await wait(downloadTree, 'completed', 120000)).files).toBe(300)
  for (let group = 0; group < 3; group++)
    for (let file = 0; file < 100; file++) {
      const path = join('tree', `group-${group}`, `file-${file}.txt`)
      expect(await hash(join(localRoot, path))).toBe(
        await hash(join(remoteRoot, path)),
      )
      expect(await hash(join(localRoot, path))).toBe(
        await hash(join(localRoot, 'received', path)),
      )
    }
  await expect(
    invoke('enqueue_transfer', { spec: { ...spec, names: ['../escape'] } }),
  ).rejects.toThrow()
  await expect(
    invoke('local_list', { root: localRootToken, path: '../escape' }),
  ).rejects.toThrow()
  await fs.symlink(remoteRoot, join(localRoot, 'outside-junction'), 'junction')
  await expect(
    invoke('local_list', { root: localRootToken, path: 'outside-junction' }),
  ).rejects.toThrow()
  const linkListing = await invoke('local_list', {
    root: localRootToken,
    path: '',
  })
  expect(
    linkListing.entries.find((entry) => entry.name === 'outside-junction')
      .blocked,
  ).toBe(true)
  const operation = (side, name, action, newName = '', confirmed = false) =>
    invoke('file_operation', {
      connectionId,
      side,
      root: localRootToken,
      directory: '/',
      name,
      operation: action,
      newName,
      mode: null,
      confirmed,
    })
  await operation('remote', 'created-folder', 'mkdir')
  await operation('remote', 'created-folder', 'rename', 'renamed-folder')
  await expect(
    operation('remote', 'renamed-folder', 'delete'),
  ).rejects.toThrow()
  await operation('remote', 'renamed-folder', 'delete', '', true)
  behavior.rejectSftp = true
  await invoke('reset_sftp', { connectionId })
  await expect(
    invoke('remote_list', { connectionId, path: '/' }),
  ).rejects.toThrow()
  await terminal.locator('.xterm-helper-textarea').focus()
  await page.keyboard.type('sftp-rejected-terminal-alive')
  await expect(terminal.locator('.xterm-screen')).toContainText(
    'sftp-rejected-terminal-alive',
  )
  behavior.rejectSftp = false
  await invoke('remote_list', { connectionId, path: '/' })
  if (process.env.NEXATERM_SFTP_STRESS === '1') {
    const file = await fs.open(join(localRoot, 'large.bin'), 'w')
    const block = Buffer.alloc(1024 * 1024)
    for (let index = 0; index < block.length; index++)
      block[index] = index % 251
    for (let index = 0; index < 1024; index++) await file.write(block)
    await file.close()
    const large = await enqueue({ names: ['large.bin'], collision: 'skip' })
    await wait(large, 'running')
    await terminal.locator('.xterm-helper-textarea').focus()
    await page.keyboard.type('during-1GiB-transfer')
    await expect(terminal.locator('.xterm-screen')).toContainText(
      'during-1GiB-transfer',
    )
    expect((await wait(large, 'completed', 180000)).bytes).toBe(1073741824)
    const largeDownload = await enqueue({
      direction: 'download',
      localDir: 'received',
      names: ['large.bin'],
      collision: 'skip',
    })
    expect((await wait(largeDownload, 'completed', 180000)).bytes).toBe(
      1073741824,
    )
    const hashes = await Promise.all([
      hash(join(localRoot, 'large.bin')),
      hash(join(remoteRoot, 'large.bin')),
      hash(join(localRoot, 'received', 'large.bin')),
    ])
    expect(new Set(hashes).size).toBe(1)
    console.log(
      `SFTP_STRESS_VERIFIED bytes=1073741824 directions=2 sha256=${hashes[0]} terminal_during_transfer=responsive`,
    )
    await fs.mkdir(join(remoteRoot, 'cancel-target'))
    const midCancel = await enqueue({
      remoteDir: '/cancel-target',
      names: ['large.bin'],
      collision: 'skip',
    })
    await expect
      .poll(
        async () =>
          (await invoke('transfer_queue')).find((job) => job.id === midCancel)
            ?.bytes || 0,
      )
      .toBeGreaterThan(1048576)
    await invoke('cancel_transfer', { id: midCancel })
    await wait(midCancel, 'cancelled')
    await expect(
      fs.stat(join(remoteRoot, 'cancel-target', 'large.bin')),
    ).rejects.toThrow()
    expect(
      (await fs.readdir(join(remoteRoot, 'cancel-target'))).filter((name) =>
        name.endsWith('.tmp'),
      ),
    ).toHaveLength(0)
    const detached = await enqueue({
      names: ['large.bin'],
      collision: 'overwrite',
    })
    await wait(detached, 'running')
    await invoke('disconnect_session', { connectionId, transferPolicy: 'keep' })
    expect((await wait(detached, 'completed', 180000)).bytes).toBe(1073741824)
    await expect
      .poll(async () => {
        try {
          await invoke('remote_list', { connectionId, path: '/' })
          return false
        } catch {
          return true
        }
      })
      .toBe(true)
    await expect.poll(() => clients.size).toBe(1)
  }
  console.log(
    'SFTP_VERIFIED recursive_files=300 roundtrip_hash=matched conflict=skip_overwrite_rename cancel_retry=passed path_escape=blocked',
  )
}
