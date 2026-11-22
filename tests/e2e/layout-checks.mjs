import { expect } from '@playwright/test'

export async function expectTerminalFits(terminal) {
  await expect
    .poll(() =>
      terminal.locator('.terminal-container').evaluate((container) => {
        const screen = container
          .querySelector('.xterm-screen')
          .getBoundingClientRect()
        const lastRow = container
          .querySelector('.xterm-rows > div:last-child')
          .getBoundingClientRect()
        let ancestor = container
        while (ancestor && !ancestor.classList.contains('main-body')) {
          const bounds = ancestor.getBoundingClientRect()
          if (
            screen.bottom > bounds.bottom + 0.5 ||
            lastRow.bottom > bounds.bottom + 0.5 ||
            screen.right > bounds.right + 0.5
          )
            return false
          ancestor = ancestor.parentElement
        }
        return screen.height > 0 && lastRow.height > 0
      }),
    )
    .toBe(true)
}

export async function exerciseLayout(page, terminal, panel, windowChanges) {
  await expectTerminalFits(terminal)
  const splitter = terminal.getByRole('separator', {
    name: 'SSH와 SFTP 영역 높이 조절',
  })
  const ssh = terminal.locator('.ssh-area')
  const original = await ssh.boundingBox()
  const handle = await splitter.boundingBox()
  const changes = windowChanges.length
  await page.mouse.move(
    handle.x + handle.width / 2,
    handle.y + handle.height / 2,
  )
  await page.mouse.down()
  await page.mouse.move(handle.x + handle.width / 2, handle.y - 75, {
    steps: 8,
  })
  await page.mouse.up()
  await expect
    .poll(async () => (await ssh.boundingBox()).height)
    .toBeLessThan(original.height - 50)
  await expect.poll(() => windowChanges.length).toBeGreaterThan(changes)
  await expectTerminalFits(terminal)
  const ratio = await splitter.getAttribute('aria-valuenow')
  await terminal.getByRole('button', { name: '파일 닫기', exact: true }).click()
  await expect(splitter).toHaveCount(0)
  await expect
    .poll(async () => (await ssh.boundingBox()).height)
    .toBeGreaterThan(original.height)
  await expectTerminalFits(terminal)
  await terminal
    .getByRole('button', { name: '파일 열기 (SFTP)', exact: true })
    .click()
  await expect(splitter).toHaveAttribute('aria-valuenow', ratio)
  await splitter.focus()
  await page.keyboard.press('ArrowUp')
  await expect(splitter).toHaveAttribute(
    'aria-valuenow',
    String(Number(ratio) - 5),
  )
  await splitter.dblclick()
  await expect(splitter).toHaveAttribute('aria-valuenow', '50')
  await expectTerminalFits(terminal)

  const local = panel.getByRole('region', { name: '로컬 파일', exact: true })
  await local
    .getByRole('button', { name: '▸ last-folder', exact: true })
    .click()
  await expect(local.getByLabel('로컬 상대 경로', { exact: true })).toHaveValue(
    'last-folder',
  )
  await terminal.getByRole('button', { name: '파일 닫기', exact: true }).click()
  await terminal
    .getByRole('button', { name: '파일 열기 (SFTP)', exact: true })
    .click()
  await expect(local.getByLabel('로컬 상대 경로', { exact: true })).toHaveValue(
    'last-folder',
  )
  await expect(
    local.getByRole('button', { name: '· remembered.txt', exact: true }),
  ).toBeVisible()
  await local
    .getByRole('button', { name: 'local 상위 폴더', exact: true })
    .click()
  await expect(local.getByLabel('로컬 상대 경로', { exact: true })).toHaveValue(
    '',
  )

  const remote = panel.getByRole('region', { name: '원격 파일', exact: true })
  await remote
    .getByRole('button', { name: '▸ scroll-fixture', exact: true })
    .click()
  const list = remote.locator('.file-table')
  await expect(remote.locator('tbody tr')).toHaveCount(100)
  await expect
    .poll(() =>
      list.evaluate(
        (el) => el.scrollHeight > el.clientHeight && el.clientHeight > 0,
      ),
    )
    .toBe(true)
  await list.hover()
  await page.mouse.wheel(0, 500)
  await expect
    .poll(() => list.evaluate((el) => el.scrollTop))
    .toBeGreaterThan(0)
  const outerFits = () =>
    page.evaluate(() => {
      const body = document.querySelector('.main-body')
      const footer = document.querySelector('footer').getBoundingClientRect()
      return (
        document.documentElement.scrollHeight <= innerHeight + 1 &&
        body.scrollHeight <= body.clientHeight + 1 &&
        body.scrollTop === 0 &&
        footer.bottom <= innerHeight + 1
      )
    })
  await expect.poll(outerFits).toBe(true)
  const viewport = await page.evaluate(() => ({
    width: innerWidth,
    height: innerHeight,
  }))
  await page.setViewportSize({ width: 1100, height: 650 })
  await expect
    .poll(() =>
      page.evaluate(() => ({ width: innerWidth, height: innerHeight })),
    )
    .toEqual({ width: 1100, height: 650 })
  await expect.poll(outerFits).toBe(true)
  await expectTerminalFits(terminal)
  await page.setViewportSize(viewport)
  await remote
    .getByRole('button', { name: 'remote 상위 폴더', exact: true })
    .click()
  await expect(
    panel.getByRole('button', { name: '· remote.txt', exact: true }),
  ).toBeVisible()
  console.log(
    'LAYOUT_VERIFIED splitter_drag=passed ssh_resize=passed independent_sftp_scroll=passed outer_scroll=absent compact_window=passed',
  )
}
