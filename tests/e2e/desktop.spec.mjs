import { test, expect, chromium } from '@playwright/test'
import { spawn } from 'node:child_process'
import { mkdtemp, readFile, mkdir, writeFile } from 'node:fs/promises'
import { serveSftp } from './sftp-fixture.mjs'
import { exerciseSftp } from './sftp-checks.mjs'
import { exerciseLayout, expectTerminalFits } from './layout-checks.mjs'
import { exerciseTheme, expectTheme } from './theme-checks.mjs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import ssh2 from 'ssh2'

test('Windows WebView: protected password, SSH trust, independent tabs and settings', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'nexaterm-desktop-test-'))
  const clients = new Set()
  const behavior = {}
  const windowChanges = []
  const localRoot = join(directory, 'local'),
    remoteRoot = join(directory, 'remote')
  await mkdir(localRoot)
  await mkdir(join(localRoot, 'last-folder'))
  await writeFile(
    join(localRoot, 'last-folder', 'remembered.txt'),
    'location fixture',
  )
  await mkdir(remoteRoot)
  await mkdir(join(remoteRoot, 'scroll-fixture'))
  await Promise.all(
    Array.from({ length: 100 }, (_, index) =>
      writeFile(
        join(remoteRoot, 'scroll-fixture', `file-${index}.txt`),
        'scroll fixture',
      ),
    ),
  )
  await writeFile(
    join(localRoot, 'upload-한글.txt'),
    'NexaTerm SFTP roundtrip 한글 데이터',
  )
  await writeFile(join(remoteRoot, 'remote.txt'), 'Remote file fixture')
  const hostKey = ssh2.utils.generateKeyPairSync('rsa', { bits: 2048 }).private
  const fixturePassword = 'fixture-desktop-password-92!'
  const server = new ssh2.Server(
    { hostKeys: [hostKey], algorithms: { serverHostKey: ['ssh-rsa'] } },
    (client) => {
      clients.add(client)
      client.on('error', () => {})
      client.on('close', () => clients.delete(client))
      client.on('authentication', (context) => {
        if (
          context.method === 'password' &&
          context.username === 'fixture' &&
          context.password === fixturePassword
        )
          context.accept()
        else context.reject()
      })
      client.on('ready', () =>
        client.on('session', (accept) => {
          const session = accept()
          serveSftp(session, remoteRoot, behavior)
          session.on('pty', (accept, reject) => {
            behavior.ptyRequests = (behavior.ptyRequests || 0) + 1
            if (behavior.sftpOnly) return reject()
            accept()
          })
          session.on('window-change', (accept, info) => {
            windowChanges.push(info)
            accept?.()
          })
          session.on('shell', (accept, reject) => {
            behavior.shellRequests = (behavior.shellRequests || 0) + 1
            if (behavior.sftpOnly) return reject()
            const stream = accept()
            stream.write(
              '\x1b[32mNexaTerm independent SSH fixture\x1b[0m\r\nfixture> ',
            )
            stream.on('data', (data) => {
              if (data.toString() === 'ack-drop-flood') {
                stream.write(
                  ('X'.repeat(78) + '\r\n').repeat(13108) +
                    'FLOW-CONTROL-COMPLETE> ',
                )
              } else if (data.toString() === 'scroll-bottom-fixture') {
                stream.write(
                  Array.from(
                    { length: 200 },
                    (_, i) => `scroll line ${i}\r\n`,
                  ).join('') + 'BOTTOM-LINE-VISIBLE> ',
                )
              } else stream.write(data)
            })
          })
        }),
      )
    },
  )
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
  const port = server.address().port
  const debugPort = 19229
  let app, browser, page
  async function launch() {
    app = spawn(resolve('src-tauri/target/debug/nexaterm.exe'), [], {
      windowsHide: true,
      env: {
        ...process.env,
        NEXATERM_TEST_DATA_DIR: directory,
        NEXATERM_TEST_LOCAL_ROOT: localRoot,
        WEBVIEW2_USER_DATA_FOLDER: join(directory, 'webview'),
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort} --remote-debugging-address=127.0.0.1`,
      },
      stdio: 'ignore',
    })
    app.on('error', () => {})
    await expect
      .poll(
        async () => {
          try {
            return (await fetch(`http://127.0.0.1:${debugPort}/json/version`))
              .ok
          } catch {
            return false
          }
        },
        { timeout: 30000 },
      )
      .toBe(true)
    await expect
      .poll(
        async () => {
          try {
            browser = await chromium.connectOverCDP(
              `http://127.0.0.1:${debugPort}`,
            )
            return true
          } catch {
            return false
          }
        },
        { timeout: 20000 },
      )
      .toBe(true)
    const page = browser.contexts()[0].pages()[0]
    await page.setViewportSize({ width: 1320, height: 860 })
    await expect(
      page.getByRole('button', { name: '새 프로필', exact: true }),
    ).toBeEnabled()
    return page
  }
  async function stop() {
    if (browser) {
      await browser.close().catch(() => {})
      browser = undefined
    }
    if (app && app.exitCode === null) {
      const exited = new Promise((resolve) => app.once('exit', resolve))
      app.kill()
      await exited
    }
    await expect
      .poll(
        async () => {
          try {
            await fetch(`http://127.0.0.1:${debugPort}/json/version`)
            return false
          } catch {
            return true
          }
        },
        { timeout: 15000 },
      )
      .toBe(true)
  }
  try {
    page = await launch()
    await expect(
      page.getByRole('heading', { name: '서버 연결', exact: true }),
    ).toBeVisible()
    for (const text of [
      'YOUR SERVERS.',
      'Windows DPAPI',
      '후속 구현',
      '내 서버를 위한 안전한 작업공간',
    ]) {
      await expect(page.locator('.workspace')).not.toContainText(text)
    }
    const initialLocation = await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke('open_local_location'),
    )
    expect(initialLocation.listing.path).toBe('')
    expect(initialLocation.root.path.endsWith(localRoot)).toBe(true)
    await page.evaluate(
      ({ root }) =>
        window.__TAURI_INTERNALS__.invoke('local_list', {
          root,
          path: 'last-folder',
          remember: true,
        }),
      { root: initialLocation.root.token },
    )
    await expect(page.getByRole('menubar').getByRole('menuitem')).toHaveText([
      '파일',
      '연결',
      '보기',
      '도구',
      '도움말',
    ])
    await expect(page.locator('aside')).not.toContainText(
      '로컬 우선 · Rust 연결 엔진',
    )
    await page.getByRole('button', { name: '새 프로필', exact: true }).click()
    await page.getByRole('button', { name: '프로필 저장', exact: true }).click()
    for (const label of ['프로필 이름', '호스트', '사용자명']) {
      await expect(page.getByLabel(label, { exact: true })).toHaveAttribute(
        'aria-invalid',
        'true',
      )
    }
    await expect(page.getByLabel('프로필 이름', { exact: true })).toBeFocused()
    await page
      .getByLabel('호스트', { exact: true })
      .fill('https://invalid host/path')
    await expect(page.locator('#profile-host-error')).toContainText(
      '공백, 경로',
    )
    await page.getByLabel('포트', { exact: true }).fill('65536')
    await expect(page.locator('#profile-port-error')).toBeVisible()
    await page.getByLabel('프로필 이름', { exact: true }).fill('가'.repeat(54))
    await expect(page.locator('#profile-name-error')).toContainText('160바이트')
    await page.getByLabel('프로필 이름', { exact: true }).fill('로컬 SSH 검증')
    await page.getByLabel('호스트', { exact: true }).fill('127.0.0.1')
    await page.getByLabel('포트', { exact: true }).fill(String(port))
    await page.getByLabel('사용자명', { exact: true }).fill('fixture')
    await page.getByLabel('그룹', { exact: true }).fill('검증')
    await expect(page.getByLabel('호스트키 알고리즘')).toHaveValue('ssh-rsa')
    await page.getByLabel('이 PC에 비밀번호 저장', { exact: true }).check()
    await expect(page.locator('#profile-password-error')).toBeVisible()
    await page.getByLabel('이 PC에 비밀번호 저장', { exact: true }).uncheck()
    await page.getByLabel('인증 방식').selectOption('privateKey')
    await expect(
      page.getByLabel('개인키 경로', { exact: true }),
    ).toHaveAttribute('aria-invalid', 'true')
    await page.getByLabel('인증 방식').selectOption('password')
    await expect(page.locator('[aria-invalid="true"]')).toHaveCount(0)
    await page.getByRole('button', { name: '프로필 저장', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    for (const save of [false, true]) {
      await page.locator('.server-connect').click()
      const checkbox = page.getByLabel('이 PC에 비밀번호 저장', { exact: true })
      await expect(checkbox).not.toBeChecked()
      await page.getByLabel('비밀번호', { exact: true }).fill(fixturePassword)
      if (save) await checkbox.check()
      await page.getByRole('button', { name: '연결', exact: true }).click()
      await expect(page.getByRole('alertdialog')).toContainText(
        '처음 연결하는 서버',
      )
      await page.getByRole('button', { name: '연결 거부', exact: true }).click()
      await expect(page.getByRole('alertdialog')).toHaveCount(0)
      if (!save) {
        const count = await page.locator('.terminal-pane').count()
        await expect(page.locator('.session-error').last()).toContainText(
          'SSH 협상/호스트키 확인 실패',
        )
        await page.locator('.xterm-helper-textarea').last().focus()
        await page.keyboard.press('Enter')
        await expect(page.getByRole('dialog')).toContainText('SSH 접속')
        await expect(page.getByLabel('비밀번호', { exact: true })).toHaveValue(
          '',
        )
        await page.getByLabel('비밀번호', { exact: true }).fill(fixturePassword)
        await page.getByRole('button', { name: '연결', exact: true }).click()
        await expect(page.getByRole('alertdialog')).toBeVisible()
        await page
          .getByRole('button', { name: '연결 거부', exact: true })
          .click()
        await expect(page.locator('.terminal-pane')).toHaveCount(count)
      }
      await page.getByRole('button', { name: '편집', exact: true }).click()
      await expect(
        page.getByLabel('이 PC에 비밀번호 저장', { exact: true }),
      ).toBeChecked({ checked: save })
      await page.keyboard.press('Escape')
    }
    await page
      .getByRole('button', { name: '콘솔 글꼴 확대', exact: true })
      .click()
    await expect(
      page.getByRole('group', { name: '콘솔 글꼴 크기' }),
    ).toContainText('15px')
    await expect
      .poll(() =>
        page.evaluate(
          async () =>
            (await window.__TAURI_INTERNALS__.invoke('get_state')).settings
              .fontSize,
        ),
      )
      .toBe(15)
    await stop()
    page = await launch()
    const restoredLocation = await page.evaluate(() =>
      window.__TAURI_INTERNALS__.invoke('open_local_location'),
    )
    expect(restoredLocation.listing.path).toBe('last-folder')
    expect(
      restoredLocation.listing.entries.some(
        (entry) => entry.name === 'remembered.txt',
      ),
    ).toBe(true)
    await page.evaluate(
      ({ root }) =>
        window.__TAURI_INTERNALS__.invoke('local_list', {
          root,
          path: '',
          remember: true,
        }),
      { root: restoredLocation.root.token },
    )
    await expect(
      page.getByRole('group', { name: '콘솔 글꼴 크기' }),
    ).toContainText('15px')
    await page
      .getByRole('button', { name: '콘솔 글꼴 축소', exact: true })
      .click()
    await expect(page.locator('.server-card')).toContainText('로컬 SSH 검증')
    const disk = await readFile(join(directory, 'settings.db'))
    expect(disk.includes(Buffer.from(fixturePassword))).toBe(false)
    await page.getByRole('button', { name: '편집', exact: true }).click()
    await expect(page.getByLabel('비밀번호', { exact: true })).toHaveValue('')
    await page
      .getByLabel('비밀번호', { exact: true })
      .fill('wrong-saved-password')
    await page.getByRole('button', { name: '프로필 저장', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await page.locator('.server-connect').click()
    await expect(page.getByRole('alertdialog')).toContainText(
      '처음 연결하는 서버',
    )
    await expect(page.getByRole('alertdialog')).toContainText('SHA256:')
    await page
      .getByRole('button', { name: '신뢰하고 연결', exact: true })
      .click()
    await expect(page.getByRole('dialog')).toContainText(
      '비밀번호 인증이 거부되었습니다',
    )
    await expect(page.getByLabel('비밀번호', { exact: true })).toHaveValue('')
    await expect(page.getByLabel('비밀번호', { exact: true })).toBeFocused()
    await expect(
      page.getByLabel('이 PC에 비밀번호 저장', { exact: true }),
    ).not.toBeChecked()
    // Reject twice, then replace the saved password and reuse the same tab.
    await page
      .getByLabel('비밀번호', { exact: true })
      .fill('wrong-retry-password')
    await page.getByRole('button', { name: '연결', exact: true }).click()
    await expect(page.getByRole('dialog')).toContainText(
      '비밀번호 인증이 거부되었습니다',
    )
    await page.getByLabel('비밀번호', { exact: true }).fill(fixturePassword)
    await page.getByLabel('이 PC에 비밀번호 저장', { exact: true }).check()
    await page.getByRole('button', { name: '연결', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(page.locator('.terminal-pane')).toHaveCount(1)
    await expect(page.locator('.session-toolbar').first()).toContainText(
      'SSH 연결됨',
    )
    const terminalA = page.locator('.terminal-pane').first()
    await expect(terminalA.locator('.xterm-screen')).toContainText(
      'NexaTerm independent SSH fixture',
    )
    await page.evaluate(() => {
      const internals = window.__TAURI_INTERNALS__
      const invoke = internals.invoke
      let dropped = false
      internals.invoke = (command, args, options) => {
        if (command === 'terminal_ack' && !dropped) {
          dropped = true
          // Reproduce the old ignored ACK failure without logging terminal data.
          return Promise.reject(new Error('fixture: dropped terminal ACK'))
        }
        return invoke(command, args, options)
      }
    })
    await terminalA.locator('.xterm-helper-textarea').focus()
    await page.keyboard.insertText('ack-drop-flood')
    await expect(terminalA.locator('.xterm-screen')).toContainText(
      'FLOW-CONTROL-COMPLETE>',
    )
    await page.keyboard.insertText('after-flood-input')
    await expect(terminalA.locator('.xterm-screen')).toContainText(
      'after-flood-input',
    )
    console.log(
      'TERMINAL_FLOW_VERIFIED lost_ack=recovered output_bytes=1048640 subsequent_input=passed',
    )
    await expect(terminalA.locator('.xterm-rows')).toHaveCSS(
      'font-size',
      '14px',
    )
    await terminalA.locator('.xterm-screen').hover()
    await page.mouse.wheel(0, -120)
    await expect(
      page.getByRole('group', { name: '콘솔 글꼴 크기' }),
    ).toContainText('14px')
    const previousResizeCount = windowChanges.length
    await page.keyboard.down('Control')
    await page.mouse.wheel(0, -120)
    await page.keyboard.up('Control')
    await expect(terminalA.locator('.xterm-rows')).toHaveCSS(
      'font-size',
      '15px',
    )
    await expect
      .poll(() => windowChanges.length)
      .toBeGreaterThan(previousResizeCount)
    await page.keyboard.down('Control')
    await page.mouse.wheel(0, 120)
    await page.keyboard.up('Control')
    await expect(terminalA.locator('.xterm-rows')).toHaveCSS(
      'font-size',
      '14px',
    )
    await terminalA.locator('.xterm-helper-textarea').focus()
    await page.keyboard.type('tab-A-unique')
    await expect(terminalA.locator('.xterm-screen')).toContainText(
      'tab-A-unique',
    )
    await page.locator('.server-connect').click()
    await expect(page.locator('.session-toolbar').nth(1)).toContainText(
      'SSH 연결됨',
    )
    await expect(page.getByRole('alertdialog')).toHaveCount(0)
    const terminalB = page.locator('.terminal-pane').nth(1)
    await expect(terminalB.locator('.xterm-screen')).not.toContainText(
      'tab-A-unique',
    )
    await terminalB.locator('.xterm-helper-textarea').focus()
    await page.keyboard.type('tab-B-unique')
    await expect(terminalB.locator('.xterm-screen')).toContainText(
      'tab-B-unique',
    )
    const tabCount = await page.locator('.terminal-pane').count()
    await page.keyboard.press('Control+Tab')
    await expect(terminalA).toBeVisible()
    await page.keyboard.press('Control+Shift+Tab')
    await expect(terminalB).toBeVisible()
    await page.keyboard.press('Control+Shift+Tab')
    await expect(terminalA).toBeVisible()
    await page.keyboard.press('Control+Tab')
    await expect(terminalB).toBeVisible()
    Array.from(clients).at(-1).end()
    await expect(terminalB.locator('.session-toolbar')).toContainText(
      '연결 종료',
    )
    await terminalB.locator('.xterm-helper-textarea').focus()
    await page.keyboard.press('Enter')
    await expect(terminalB.locator('.session-toolbar')).toContainText(
      'SSH 연결됨',
    )
    await expect(page.locator('.terminal-pane')).toHaveCount(tabCount)
    await expect(page.getByRole('alertdialog')).toHaveCount(0)
    await terminalB.locator('.xterm-helper-textarea').focus()
    // Three tabs distinguish forward and reverse cycling, including wraparound.
    await page.locator('.server-connect').click()
    const terminalC = page.locator('.terminal-pane').nth(2)
    await expect(terminalC.locator('.session-toolbar')).toContainText(
      'SSH 연결됨',
    )
    // Move live tabs without replacing terminal DOM nodes or SSH connections.
    const originalOrder = await page
      .locator('.tab')
      .evaluateAll((elements) =>
        elements.map((element) => element.dataset.tabId),
      )
    const tabById = (id) => page.locator(`[data-tab-id="${id}"]`)
    const order = () =>
      page
        .locator('.tab')
        .evaluateAll((elements) =>
          elements.map((element) => element.dataset.tabId),
        )
    const activeTabId = () =>
      page.locator('.tab.active').getAttribute('data-tab-id')
    const terminalNodes = await page.locator('.terminal-pane').elementHandles()
    const terminalContents = await Promise.all(
      terminalNodes.map((node) =>
        node.$eval('.xterm-screen', (element) => element.textContent),
      ),
    )
    async function dragTab(id, targetId, after, cancel) {
      const source = await tabById(id).locator('.tab-select').boundingBox()
      const target = await tabById(targetId).boundingBox()
      await page.mouse.move(
        source.x + source.width / 2,
        source.y + source.height / 2,
      )
      await page.mouse.down()
      await page.mouse.move(
        target.x + (after ? target.width - 5 : 5),
        target.y + target.height / 2,
        { steps: 12 },
      )
      if (cancel === 'escape') await page.keyboard.press('Escape')
      if (cancel === 'outside') await page.mouse.move(target.x, target.y + 100)
      await page.mouse.up()
    }
    await dragTab(originalOrder[0], originalOrder[2], true)
    await expect
      .poll(order)
      .toEqual([originalOrder[1], originalOrder[2], originalOrder[0]])
    await expect.poll(activeTabId).toBe(originalOrder[2])
    await expect.poll(() => clients.size).toBe(3)
    for (const [index, node] of terminalNodes.entries()) {
      expect(await node.evaluate((element) => element.isConnected)).toBe(true)
      expect(
        await node.$eval('.xterm-screen', (element) => element.textContent),
      ).toBe(terminalContents[index])
    }
    await expect(
      page.locator('.terminal-pane').nth(2).locator('.xterm-screen'),
    ).toContainText('tab-A-unique')
    await page.keyboard.press('Control+Tab')
    await expect.poll(activeTabId).toBe(originalOrder[0])
    await page.keyboard.press('Control+Shift+Tab')
    await expect.poll(activeTabId).toBe(originalOrder[2])
    await dragTab(originalOrder[0], originalOrder[1], false, 'escape')
    await expect
      .poll(order)
      .toEqual([originalOrder[1], originalOrder[2], originalOrder[0]])
    await expect.poll(activeTabId).toBe(originalOrder[2])
    await dragTab(originalOrder[0], originalOrder[1], false, 'outside')
    await expect
      .poll(order)
      .toEqual([originalOrder[1], originalOrder[2], originalOrder[0]])
    await expect.poll(activeTabId).toBe(originalOrder[2])
    await dragTab(originalOrder[0], originalOrder[1], false)
    await expect.poll(order).toEqual(originalOrder)
    await expect.poll(activeTabId).toBe(originalOrder[2])
    await terminalC.locator('.xterm-helper-textarea').focus()
    console.log(
      'TAB_DRAG_VERIFIED forward=passed backward=passed cancel=passed sessions=preserved terminal_nodes=preserved',
    )
    await page.keyboard.press('Control+Shift+Tab')
    await expect(terminalB).toBeVisible()
    await page.keyboard.press('Control+Tab')
    await expect(terminalC).toBeVisible()
    await page.keyboard.press('Control+Tab')
    await expect(terminalA).toBeVisible()
    await page.keyboard.press('Control+Shift+Tab')
    await expect(terminalC).toBeVisible()
    page.once('dialog', (dialog) => dialog.accept())
    await page.locator('.tab-close').nth(2).click()
    await expect.poll(() => clients.size).toBe(2)
    await expect(page.locator('.terminal-pane')).toHaveCount(tabCount)
    await terminalB.locator('.xterm-helper-textarea').focus()
    // A strict algorithm mismatch must leave the tab retryable.
    await page.getByRole('button', { name: '편집', exact: true }).click()
    await page.getByLabel('호스트키 알고리즘').selectOption('ssh-ed25519')
    await page.getByRole('button', { name: '프로필 저장', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    Array.from(clients).at(-1).end()
    await expect(terminalB.locator('.session-toolbar')).toContainText(
      '연결 종료',
    )
    await terminalB.locator('.xterm-helper-textarea').focus()
    await page.keyboard.press('Enter')
    await expect(terminalB.locator('.session-error')).toContainText(
      'SSH 협상/호스트키 확인 실패',
    )
    await page.getByRole('button', { name: '편집', exact: true }).click()
    await expect(page.getByLabel('호스트키 알고리즘')).toHaveValue(
      'ssh-ed25519',
    )
    await page.getByLabel('호스트키 알고리즘').selectOption('ssh-rsa')
    await page.getByRole('button', { name: '프로필 저장', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await terminalB.locator('.xterm-helper-textarea').focus()
    await page.keyboard.press('Enter')
    await expect(terminalB.locator('.session-toolbar')).toContainText(
      'SSH 연결됨',
    )
    await expect(page.locator('.terminal-pane')).toHaveCount(tabCount)
    console.log(
      'SSH_RSA_RETRY_SHORTCUTS_VERIFIED rsa_only=passed disconnected_enter=passed error_enter=passed three_tab_forward_reverse=passed',
    )
    await terminalB.locator('.xterm-helper-textarea').focus()
    await page.keyboard.insertText('scroll-bottom-fixture')
    await expect(terminalB.locator('.xterm-rows > div').last()).toContainText(
      'BOTTOM-LINE-VISIBLE>',
    )
    await expectTerminalFits(terminalB)
    await terminalB.locator('.xterm-screen').hover()
    await page.mouse.wheel(0, -500)
    await expect(terminalB.locator('.xterm-rows')).not.toContainText(
      'BOTTOM-LINE-VISIBLE>',
    )
    await page.mouse.wheel(0, 5000)
    await expect(terminalB.locator('.xterm-rows > div').last()).toContainText(
      'BOTTOM-LINE-VISIBLE>',
    )
    await expectTerminalFits(terminalB)
    await expect(terminalA.locator('.xterm-screen')).not.toContainText(
      'tab-B-unique',
    )
    await expect(page.locator('footer')).toContainText('활성 연결 2')
    await page
      .getByRole('button', { name: '콘솔 글꼴 확대', exact: true })
      .click()
    await expect(terminalA.locator('.xterm-rows')).toHaveCSS(
      'font-size',
      '15px',
    )
    await expect(terminalB.locator('.xterm-rows')).toHaveCSS(
      'font-size',
      '15px',
    )
    await page
      .locator('.terminal-pane')
      .nth(1)
      .getByRole('button', { name: '파일 열기 (SFTP)', exact: true })
      .click()
    const panel = page
      .locator('.terminal-pane')
      .nth(1)
      .getByRole('region', { name: 'SFTP 파일 관리자' })
    await expect(
      panel.getByRole('button', { name: '· remote.txt', exact: true }),
    ).toBeVisible()
    const localPane = panel.getByRole('region', {
      name: '로컬 파일',
      exact: true,
    })
    await expect(localPane.locator('.root-label')).toHaveText(localRoot)
    await expect(localPane.locator('.root-label')).toHaveAttribute(
      'title',
      localRoot,
    )
    await expect(panel.locator('.sftp-note')).toHaveCount(0)
    await expect(panel).not.toContainText('폴더는 재귀 전송하며')
    await expect(localPane.locator('.empty')).toHaveCount(0)
    await exerciseLayout(page, terminalB, panel, windowChanges)
    await panel
      .getByLabel('local upload-한글.txt 선택', { exact: true })
      .check()
    await panel
      .getByRole('button', { name: '업로드 → (1)', exact: true })
      .click()
    await expect(
      page.locator('.transfer-job[data-status="completed"]'),
    ).toHaveCount(1)
    expect(await readFile(join(remoteRoot, 'upload-한글.txt'), 'utf8')).toBe(
      'NexaTerm SFTP roundtrip 한글 데이터',
    )
    await terminalB.locator('.xterm-helper-textarea').focus()
    await page.keyboard.type('terminal-with-sftp')
    await expect(terminalB.locator('.xterm-screen')).toContainText(
      'terminal-with-sftp',
    )
    await panel.getByRole('button', { name: '새로고침', exact: true }).click()
    await panel.getByLabel('remote remote.txt 선택', { exact: true }).check()
    await panel
      .getByRole('button', { name: '← 다운로드 (1)', exact: true })
      .click()
    await expect(
      page.locator('.transfer-job[data-status="completed"]'),
    ).toHaveCount(2)
    expect(await readFile(join(localRoot, 'remote.txt'), 'utf8')).toBe(
      'Remote file fixture',
    )
    await page.screenshot({ path: 'test-results/desktop-sftp.png' })
    await page.getByRole('menuitem', { name: '파일', exact: true }).click()
    await page
      .getByRole('menuitem', { name: '설정 백업…', exact: true })
      .click()
    await expect(page.getByRole('dialog')).toContainText('암호화 백업')
    await expect(
      page.getByLabel('저장된 비밀번호 / passphrase 포함'),
    ).not.toBeChecked()
    await page.getByRole('button', { name: '취소', exact: true }).click()
    await page.getByRole('menuitem', { name: '도구', exact: true }).click()
    await page
      .getByRole('menuitem', { name: '환경 설정…', exact: true })
      .click()
    await page.getByLabel('터미널 글자 크기').fill('16')
    await page.getByRole('button', { name: '설정 저장', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(page.locator('footer')).toContainText('16px')
    await exerciseSftp(page, {
      localRoot,
      remoteRoot,
      terminal: terminalB,
      behavior,
      clients,
    })
    await mkdir('test-results', { recursive: true })
    await page.screenshot({ path: 'test-results/desktop-ssh.png' })
    await page.getByRole('menuitem', { name: '도구', exact: true }).click()
    await page
      .getByRole('menuitem', { name: '환경 설정…', exact: true })
      .click()
    await page.getByRole('dialog').getByRole('combobox').selectOption('light')
    await page.getByRole('button', { name: '설정 저장', exact: true }).click()
    await expect(page.locator('.workspace')).toHaveAttribute(
      'data-theme',
      'light',
    )
    await page.screenshot({ path: 'test-results/desktop-brand-light.png' })
    await page.getByRole('menuitem', { name: '도구', exact: true }).click()
    await page
      .getByRole('menuitem', { name: '환경 설정…', exact: true })
      .click()
    await page.getByRole('dialog').getByRole('combobox').selectOption('dark')
    await page.getByRole('button', { name: '설정 저장', exact: true }).click()
    await exerciseTheme(page)
    await expect(
      page.getByRole('button', { name: '시작', exact: true }),
    ).toHaveCount(0)
    await page.screenshot({ path: 'test-results/desktop-workspace.png' })
    behavior.sftpOnly = true
    const terminalRequests = [behavior.ptyRequests, behavior.shellRequests]
    await page.getByRole('button', { name: '편집', exact: true }).click()
    await page.getByLabel('접속 방식').selectOption('sftp')
    await page.getByRole('button', { name: '프로필 저장', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(page.locator('.server-name')).toHaveText('로컬 SSH 검증')
    await stop()
    page = await launch()
    await page.emulateMedia({ colorScheme: 'light' })
    await expectTheme(page, 'light')
    await page.emulateMedia({ colorScheme: 'dark' })
    await expectTheme(page, 'dark')
    await page.getByRole('menuitem', { name: '도구', exact: true }).click()
    await page
      .getByRole('menuitem', { name: '환경 설정…', exact: true })
      .click()
    await expect(page.getByLabel('테마', { exact: true })).toHaveValue('system')
    await page.getByRole('button', { name: '취소', exact: true }).click()
    await page.emulateMedia({ colorScheme: null })
    await page.getByRole('button', { name: '편집', exact: true }).click()
    await expect(page.getByLabel('접속 방식')).toHaveValue('sftp')
    await page
      .getByLabel('비밀번호', { exact: true })
      .fill('wrong-sftp-password')
    await page.getByRole('button', { name: '프로필 저장', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await page.locator('.server-connect').click()
    await expect(page.getByRole('dialog')).toContainText(
      '비밀번호 인증이 거부되었습니다',
    )
    await page.getByRole('button', { name: '취소', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await page
      .getByRole('button', { name: '재접속 (Enter)', exact: true })
      .click()
    await expect(page.getByRole('dialog')).toContainText(
      '비밀번호 인증이 거부되었습니다',
    )
    await page.getByLabel('비밀번호', { exact: true }).fill(fixturePassword)
    await page.getByLabel('이 PC에 비밀번호 저장', { exact: true }).check()
    await page.getByRole('button', { name: '연결', exact: true }).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(page.locator('.terminal-pane')).toHaveCount(1)
    const sftpTab = page.locator('.terminal-pane').last()
    await expect(sftpTab.locator('.session-toolbar')).toContainText(
      'SFTP 연결됨',
    )
    await expect(sftpTab.locator('.terminal-container')).toHaveCount(0)
    await expect(sftpTab.getByRole('separator')).toHaveCount(0)
    await expect(
      sftpTab.getByRole('button', { name: '화면 지우기', exact: true }),
    ).toHaveCount(0)
    expect([behavior.ptyRequests, behavior.shellRequests]).toEqual(
      terminalRequests,
    )
    const sftpOnlyPanel = sftpTab.getByRole('region', {
      name: 'SFTP 파일 관리자',
    })
    await sftpOnlyPanel.getByLabel('동일 이름').selectOption('overwrite')
    await expect(
      sftpOnlyPanel
        .getByRole('region', { name: '원격 파일', exact: true })
        .getByRole('button', { name: '· remote.txt', exact: true }),
    ).toBeVisible()
    await sftpOnlyPanel
      .getByLabel('local upload-한글.txt 선택', { exact: true })
      .check()
    await sftpOnlyPanel
      .getByRole('button', { name: '업로드 → (1)', exact: true })
      .click()
    await expect(
      page.locator('.transfer-job[data-status="completed"]'),
    ).toHaveCount(1)
    expect(await readFile(join(remoteRoot, 'upload-한글.txt'), 'utf8')).toBe(
      await readFile(join(localRoot, 'upload-한글.txt'), 'utf8'),
    )
    await sftpOnlyPanel
      .getByLabel('remote remote.txt 선택', { exact: true })
      .check()
    await sftpOnlyPanel
      .getByRole('button', { name: '← 다운로드 (1)', exact: true })
      .click()
    await expect(
      page.locator('.transfer-job[data-status="completed"]'),
    ).toHaveCount(2)
    expect(await readFile(join(localRoot, 'remote.txt'), 'utf8')).toBe(
      await readFile(join(remoteRoot, 'remote.txt'), 'utf8'),
    )
    await page.screenshot({ path: 'test-results/desktop-sftp-only.png' })
    page.once('dialog', (dialog) => dialog.accept())
    await sftpTab
      .getByRole('button', { name: '연결 종료', exact: true })
      .click()
    await expect(sftpTab).toHaveCount(0)
    behavior.rejectSftp = true
    await page.locator('.server-connect').click()
    await expect(page.locator('.session-error').last()).toContainText('SFTP')
    expect([behavior.ptyRequests, behavior.shellRequests]).toEqual(
      terminalRequests,
    )
    console.log(
      'SFTP_ONLY_VERIFIED shell_and_pty=never_requested restart_mode=preserved upload=passed subsystem_rejection=reported',
    )
    behavior.sftpOnly = false
    behavior.rejectSftp = false
    await page.getByRole('button', { name: '편집', exact: true }).click()
    await page.getByLabel('접속 방식').selectOption('ssh')
    await page.getByRole('button', { name: '프로필 저장', exact: true }).click()
    await page.locator('.server-connect').click()
    await expect(
      page.locator('.terminal-pane:not(.hidden) .xterm'),
    ).toBeVisible()
    await page.getByRole('menuitem', { name: '도구', exact: true }).click()
    await page
      .getByRole('menuitem', { name: '환경 설정…', exact: true })
      .click()
    await page
      .getByRole('checkbox', {
        name: '활성 연결이 있을 때 앱 종료 확인창 표시',
      })
      .uncheck()
    await page.getByRole('button', { name: '설정 저장', exact: true }).click()
    await page
      .locator('.terminal-pane:not(.hidden) .xterm-helper-textarea')
      .focus()
    await page.keyboard.press('Alt+F4')
    await expect.poll(() => app.exitCode, { timeout: 15000 }).not.toBeNull()
    console.log(
      'ALT_F4_VERIFIED terminal_focus=closed confirmation_disabled=respected',
    )
    await stop()
    page = await launch()
    await expect(page.locator('footer')).toContainText('활성 연결 0')
    await page.getByRole('textbox', { name: '서버 검색' }).focus()
    await page.keyboard.press('Alt+F4')
    await expect.poll(() => app.exitCode, { timeout: 15000 }).not.toBeNull()
    console.log('ALT_F4_NO_SESSION_VERIFIED app=closed')
    console.log(
      'DESKTOP_VERIFIED profiles=1 independent_connections=2 plaintext_password=absent',
    )
  } catch (error) {
    if (page && !page.isClosed())
      console.log(
        'SFTP_DIAGNOSTIC',
        await page.evaluate(async () => ({
          jobs: await window.__TAURI_INTERNALS__.invoke('transfer_queue'),
          focus: document.activeElement?.outerHTML.slice(0, 400),
          tabs: Array.from(document.querySelectorAll('.tab')).map(
            (e) => e.outerHTML,
          ),
          body: document.body.innerText.slice(-2000),
          errors: Array.from(document.querySelectorAll('[role="alert"]')).map(
            (e) => e.textContent,
          ),
        })),
      )
    throw error
  } finally {
    await stop()
    for (const client of clients) client.end()
    await new Promise((resolve) => server.close(resolve))
  }
})
