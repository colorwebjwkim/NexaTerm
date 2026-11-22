import { chromium, expect } from '@playwright/test'
import { spawn } from 'node:child_process'
import { copyFile, mkdtemp } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'

// Node.js runs this verification harness; the app receives only Windows paths.
const source = resolve(process.argv[2] || 'src-tauri/target/debug/nexaterm.exe')
const directory = await mkdtemp(join(tmpdir(), 'nexaterm-runtime-check-'))
const executable = join(directory, basename(source))
await copyFile(source, executable)
const env = Object.fromEntries(
  Object.entries(process.env).filter(([key]) =>
    /^(SystemRoot|WINDIR|USERPROFILE|LOCALAPPDATA|APPDATA|TEMP|TMP|ProgramFiles|ProgramFiles\(x86\)|ProgramData|COMSPEC|HOMEDRIVE|HOMEPATH|USERNAME|USERDOMAIN)$/i.test(
      key,
    ),
  ),
)
env.PATH = `${process.env.SystemRoot}\\System32;${process.env.SystemRoot}`
env.NEXATERM_TEST_DATA_DIR = join(directory, 'data')
env.WEBVIEW2_USER_DATA_FOLDER = join(directory, 'webview')
env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS =
  '--remote-debugging-port=19239 --remote-debugging-address=127.0.0.1'
const app = spawn(executable, [], {
  cwd: directory,
  env,
  windowsHide: true,
  stdio: 'ignore',
})
let browser
try {
  await expect
    .poll(
      async () => {
        if (app.exitCode !== null)
          throw new Error(`App exited: ${app.exitCode}`)
        try {
          browser = await chromium.connectOverCDP('http://127.0.0.1:19239')
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
  const result = await page.evaluate(async () => {
    const state = await window.__TAURI_INTERNALS__.invoke('get_state')
    return {
      nativeBridge: Boolean(state.settings),
      nodeRequire: typeof globalThis.require,
    }
  })
  expect(result.nativeBridge).toBe(true)
  expect(result.nodeRequire).toBe('undefined')
  console.log(
    JSON.stringify(
      {
        result: 'RUNTIME_CHECK_PASSED',
        source,
        copiedExecutableOnly: true,
        cwd: directory,
        childPath: env.PATH,
        ...result,
        limitation:
          'Existing Windows installation with WebView2; not a clean PC test',
      },
      null,
      2,
    ),
  )
} finally {
  await browser?.close().catch(() => {})
  if (app.exitCode === null) {
    const exited = new Promise((resolve) => app.once('exit', resolve))
    app.kill()
    await exited
  }
}
