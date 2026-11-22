import { chromium, expect } from '@playwright/test'
import { spawn } from 'node:child_process'
import { resolve } from 'node:path'
import { stat, mkdtemp, readFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const executable = resolve(
  process.argv[2] || 'src-tauri/target/release/nexaterm.exe',
)
const { version } = JSON.parse(
  await readFile(new URL('../package.json', import.meta.url), 'utf8'),
)
const metadata = await stat(executable)
const port = 19230
const webviewDirectory = await mkdtemp(
  join(tmpdir(), 'nexaterm-release-webview-'),
)
const app = spawn(executable, [], {
  windowsHide: true,
  env: {
    ...process.env,
    WEBVIEW2_USER_DATA_FOLDER: webviewDirectory,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`,
  },
  stdio: 'ignore',
})
let browser
try {
  await expect
    .poll(
      async () => {
        try {
          browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`)
          return true
        } catch {
          return false
        }
      },
      { timeout: 30000 },
    )
    .toBe(true)
  const page = browser.contexts()[0].pages()[0]
  await expect(
    page.getByRole('button', { name: '새 프로필', exact: true }),
  ).toBeEnabled()
  await expect(page.getByRole('heading', { name: /서버 연결/ })).toBeVisible()
  await expect(page.locator('header img')).toHaveCount(0)
  await expect(page.getByRole('menubar').getByRole('menuitem')).toHaveText([
    '파일',
    '연결',
    '보기',
    '도구',
    '도움말',
  ])
  await expect(page.locator('footer .dot')).toHaveCount(0)
  await expect(page.locator('footer')).toContainText('활성 연결')
  const mainWidth = await page
    .locator('main')
    .evaluate((main) => main.clientWidth)
  await page
    .getByRole('button', { name: '서버 목록 접기', exact: true })
    .click()
  await expect(page.locator('#server-sidebar')).toBeHidden()
  await expect(page.getByRole('textbox', { name: '서버 검색' })).toBeHidden()
  expect(await page.locator('main').evaluate((main) => main.clientWidth)).toBe(
    mainWidth + 285,
  )
  await page
    .getByRole('button', { name: '서버 목록 펼치기', exact: true })
    .click()
  await expect(page.getByRole('textbox', { name: '서버 검색' })).toBeVisible()
  expect(await page.locator('main').evaluate((main) => main.clientWidth)).toBe(
    mainWidth,
  )
  await page.getByRole('menuitem', { name: '도움말', exact: true }).click()
  await page
    .getByRole('menuitem', { name: 'NexaTerm 정보', exact: true })
    .click()
  await expect(page.getByRole('dialog')).toContainText(`버전 ${version}`)
  await page.keyboard.press('Escape')
  await page.getByRole('button', { name: '새 프로필', exact: true }).click()
  await expect(page.getByLabel('호스트키 알고리즘')).toHaveValue('ssh-rsa')
  await expect(
    page.getByLabel('호스트키 알고리즘').locator('option'),
  ).toHaveCount(8)
  await page.keyboard.press('Escape')
  await page.keyboard.press('F1')
  await expect(page.getByRole('dialog')).toContainText('Ctrl+Shift+Tab')
  await page.keyboard.press('Escape')
  expect(await page.evaluate(() => typeof globalThis.require)).toBe('undefined')
  console.log(
    `RELEASE_SMOKE_OK bytes=${metadata.size} native_bridge=ready node_access=absent`,
  )
} finally {
  await browser?.close().catch(() => {})
  if (app.exitCode === null) {
    const exited = new Promise((resolve) => app.once('exit', resolve))
    app.kill()
    await exited
  }
}
