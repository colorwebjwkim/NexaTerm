import { chromium, expect } from '@playwright/test'
import { spawn, execFileSync } from 'node:child_process'
import { mkdtemp } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const directory = await mkdtemp(join(tmpdir(), 'nexaterm-documents-smoke-'))
const env = {
  ...process.env,
  NEXATERM_TEST_DATA_DIR: directory,
  WEBVIEW2_USER_DATA_FOLDER: join(directory, 'webview'),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:
    '--remote-debugging-port=19231 --remote-debugging-address=127.0.0.1',
}
delete env.NEXATERM_TEST_LOCAL_ROOT
const expected = execFileSync(
  'powershell.exe',
  [
    '-NoProfile',
    '-Command',
    '[Console]::OutputEncoding = [Text.UTF8Encoding]::new(); [Environment]::GetFolderPath("MyDocuments")',
  ],
  { encoding: 'utf8', windowsHide: true },
).trim()
const app = spawn(resolve('src-tauri/target/debug/nexaterm.exe'), [], {
  env,
  windowsHide: true,
  stdio: 'ignore',
})
let browser
try {
  await expect
    .poll(
      async () => {
        try {
          browser = await chromium.connectOverCDP('http://127.0.0.1:19231')
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
  const location = await page.evaluate(() =>
    window.__TAURI_INTERNALS__.invoke('open_local_location'),
  )
  const actual = location.root.path
    .replace(/^\\\\\?\\UNC\\/i, '\\\\')
    .replace(/^\\\\\?\\(?=[a-z]:\\)/i, '')
  expect(actual.toLowerCase()).toBe(expected.toLowerCase())
  expect(location.listing.path).toBe('')
  console.log(
    'LOCAL_DEFAULT_VERIFIED windows_known_documents=matched isolated_settings=true',
  )
} finally {
  await browser?.close().catch(() => {})
  if (app.exitCode === null) {
    const exited = new Promise((resolve) => app.once('exit', resolve))
    app.kill()
    await exited
  }
}
