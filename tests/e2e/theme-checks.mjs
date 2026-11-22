import { expect } from '@playwright/test'

export async function chooseTheme(page, theme) {
  await page.getByRole('menuitem', { name: '도구', exact: true }).click()
  await page.getByRole('menuitem', { name: '환경 설정…', exact: true }).click()
  await page.getByLabel('테마', { exact: true }).selectOption(theme)
  await page.getByRole('button', { name: '설정 저장', exact: true }).click()
  await expect(page.getByRole('dialog')).toHaveCount(0)
}

export async function expectTheme(page, theme) {
  await expect(page.locator('.workspace')).toHaveAttribute('data-theme', theme)
  await expect(page.locator('.workspace')).toHaveCSS('color-scheme', theme)
  const terminals = page.locator(
    '.terminal-container .xterm-scrollable-element',
  )
  if (await page.locator('.terminal-container').count()) {
    await expect(terminals).not.toHaveCount(0)
    for (const terminal of await terminals.all()) {
      await expect(terminal).toHaveCSS(
        'background-color',
        theme === 'light' ? 'rgb(243, 245, 251)' : 'rgb(13, 16, 25)',
      )
    }
  }
  await expect(page.locator('.toast')).toHaveCount(0)
}

export async function exerciseTheme(page) {
  // Emulate the OS preference without changing the user's Windows settings.
  await page.emulateMedia({ colorScheme: 'dark' })
  await chooseTheme(page, 'system')
  await expectTheme(page, 'dark')
  await page.emulateMedia({ colorScheme: 'light' })
  await expectTheme(page, 'light')
  await page.emulateMedia({ colorScheme: 'dark' })
  await expectTheme(page, 'dark')

  await chooseTheme(page, 'light')
  await expectTheme(page, 'light')
  await page.emulateMedia({ colorScheme: 'light' })
  await page.emulateMedia({ colorScheme: 'dark' })
  await expectTheme(page, 'light')
  await chooseTheme(page, 'dark')
  await page.emulateMedia({ colorScheme: 'light' })
  await expectTheme(page, 'dark')

  await page.getByRole('menuitem', { name: '도구', exact: true }).click()
  await page.getByRole('menuitem', { name: '환경 설정…', exact: true }).click()
  await page.getByLabel('테마', { exact: true }).selectOption('light')
  await page.getByRole('button', { name: '취소', exact: true }).click()
  await expectTheme(page, 'dark')
  await page.getByRole('menuitem', { name: '보기', exact: true }).click()
  await page
    .getByRole('menuitemcheckbox', { name: 'Windows 테마 따르기' })
    .click()
  await expectTheme(page, 'light')
  await page.getByRole('menuitem', { name: '보기', exact: true }).click()
  await expect(
    page.getByRole('menuitemcheckbox', { name: 'Windows 테마 따르기' }),
  ).toHaveAttribute('aria-checked', 'true')
  await page.keyboard.press('Escape')
  await page.emulateMedia({ colorScheme: null })
  const windowsTheme = await page.evaluate(() =>
    matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light',
  )
  await expectTheme(page, windowsTheme)
  console.log(`WINDOWS_THEME_DEFAULT ${windowsTheme}`)
}
