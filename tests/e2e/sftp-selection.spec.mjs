import { test, expect } from '@playwright/test'
import { createServer } from 'vite'

test.use({ launchOptions: { channel: 'chrome' }, actionTimeout: 5000 })
test.setTimeout(60000)

let server
test.beforeAll(async () => {
  server = await createServer({
    server: { port: 0, strictPort: false },
    plugins: [
      {
        name: 'selection-fixture',
        resolveId(id) {
          if (id === '/selection-entry.js') return id
        },
        load(id) {
          if (id === '/selection-entry.js')
            return `
        import { mount } from 'svelte'
        import SftpPane from '/components/SftpPane.svelte'
        import '/styles/app.css'
        const entries = [
          {name:'folder', directory:true},
          {name:'a.txt'}, {name:'blocked', blocked:true},
          {name:'b.txt'}, {name:'c.txt'}, {name:'.hidden'},
          ...Array.from({length:40},(_,i)=>({name:'more-'+i+'.txt'}))
        ].map(e=>({directory:false,blocked:false,size:100,modified:0,mode:null,...e}))
        window.__TAURI_INTERNALS__ = { invoke: async (command,args) => {
          if(command==='open_local_location') return {
            root:{token:'test',path:'C:/test'}, listing:{path:'',entries}}
          if(command==='remote_list' || command==='local_list')
            return {path:args.path,entries: args.path.endsWith('folder') ? [] : entries}
          return null
        }}
        mount(SftpPane,{target:document.body,props:{connectionId:'test',changed:()=>{}}})
`
        },
        configureServer(vite) {
          vite.middlewares.use('/selection-test', (_req, res) => {
            res.setHeader('Content-Type', 'text/html')
            res.end(
              '<!doctype html><html><body style="height:500px;margin:0"><script type="module" src="/selection-entry.js"></script></body></html>',
            )
          })
        },
      },
    ],
  })
  await server.listen()
})
test.afterAll(async () => {
  await server?.close()
})

test('SFTP rows support Explorer selection in both panels', async ({
  page,
}) => {
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(
    `http://127.0.0.1:${server.httpServer.address().port}/selection-test`,
  )
  for (const side of ['local', 'remote']) {
    const pane = page.getByRole('region', {
      name: side === 'local' ? '로컬 파일' : '원격 파일',
      exact: true,
    })
    const row = (name) =>
      pane.locator('tr').filter({
        has: page.getByRole('checkbox', {
          name: `${side} ${name} 선택`,
          exact: true,
        }),
      })
    const selected = async (names) => {
      await expect(pane.locator('tbody tr.selected')).toHaveCount(names.length)
      for (const name of names)
        await expect(
          page.getByRole('checkbox', {
            name: `${side} ${name} 선택`,
            exact: true,
          }),
        ).toBeChecked()
    }
    await row('a.txt').locator('td').nth(2).click()
    await selected(['a.txt'])
    await row('c.txt').click({ modifiers: ['Control'] })
    await selected(['a.txt', 'c.txt'])
    await row('a.txt').click({ modifiers: ['Control'] })
    await selected(['c.txt'])
    await row('a.txt').click()
    await row('c.txt').click({ modifiers: ['Shift'] })
    await selected(['a.txt', 'b.txt', 'c.txt'])
    await row('b.txt').click({ modifiers: ['Shift'] })
    await selected(['a.txt', 'b.txt'])
    await row('c.txt').click({ modifiers: ['Control', 'Shift'] })
    await selected(['a.txt', 'b.txt', 'c.txt'])
    await row('blocked').click()
    await selected(['a.txt', 'b.txt', 'c.txt'])
    await pane
      .getByRole('checkbox', { name: `${side} b.txt 선택`, exact: true })
      .click()
    await selected(['a.txt', 'c.txt'])
    await pane
      .getByRole('checkbox', { name: `${side} b.txt 선택`, exact: true })
      .click()
    await selected(['a.txt', 'b.txt', 'c.txt'])
    await row('folder').click()
    await selected(['folder'])
    const a = await row('a.txt').boundingBox()
    const c = await row('c.txt').boundingBox()
    await page.mouse.move(a.x + a.width - 20, a.y + 2)
    await page.mouse.down()
    await page.mouse.move(c.x + c.width - 50, c.y + c.height - 2, { steps: 8 })
    await expect(pane.locator('.file-selection-box')).toBeVisible()
    await page.mouse.up()
    await selected(['a.txt', 'b.txt', 'c.txt'])
    await expect(pane.locator('.file-selection-box')).toBeHidden()
    // Ctrl drag toggles the intersected rows relative to the initial selection.
    await page.keyboard.down('Control')
    await page.mouse.move(a.x + a.width - 20, a.y + 2)
    await page.mouse.down()
    await page.mouse.move(c.x + c.width - 50, c.y + c.height - 2, { steps: 8 })
    await page.mouse.up()
    await page.keyboard.up('Control')
    await selected([])
    // Scrolling while dragging must extend selection beyond the visible rows.
    await row('a.txt').click()
    const viewport = pane.locator('.file-table')
    const bounds = await viewport.boundingBox()
    const start = await row('a.txt').boundingBox()
    await page.mouse.move(start.x + start.width - 30, start.y + 2)
    await page.mouse.down()
    await page.mouse.move(
      bounds.x + bounds.width - 50,
      bounds.y + bounds.height + 10,
      { steps: 5 },
    )
    await expect
      .poll(() => viewport.evaluate((el) => el.scrollTop))
      .toBeGreaterThan(40)
    await page.mouse.up()
    await expect(pane.locator('.file-selection-box')).toBeHidden()
    await row('folder').dblclick()
    await expect(pane.locator('tbody tr')).toHaveCount(0)
    // Empty area click clears without altering the opposite panel.
    await pane.getByRole('button', { name: `${side} 상위 폴더` }).click()
    await row('a.txt').click()
    const table = pane.locator('.file-table')
    await table.evaluate((el) => {
      el.style.minHeight = '1800px'
    })
    await table.click({ position: { x: 100, y: 1750 } })
    await selected([])
  }
  expect(errors).toEqual([])
})

test('SFTP keyboard focus and selection follow Explorer conventions', async ({
  page,
}) => {
  await page.goto(
    `http://127.0.0.1:${server.httpServer.address().port}/selection-test`,
  )
  for (const side of ['local', 'remote']) {
    const pane = page.getByRole('region', {
      name: side === 'local' ? '로컬 파일' : '원격 파일',
      exact: true,
    })
    const grid = pane.getByRole('grid')
    const selected = async (names) => {
      await expect(pane.locator('tbody tr.selected')).toHaveCount(names.length)
      for (const name of names)
        await expect(
          pane.getByRole('checkbox', {
            name: `${side} ${name} 선택`,
            exact: true,
          }),
        ).toBeChecked()
    }
    const focused = async (name) => {
      await expect(grid).toBeFocused()
      await expect(pane.locator('tr.focused')).toHaveAttribute(
        'data-file-name',
        name,
      )
      const activeId = await grid.getAttribute('aria-activedescendant')
      await expect(pane.locator('tr.focused')).toHaveAttribute('id', activeId)
    }
    await grid.focus()
    await page.keyboard.press('Home')
    await selected(['folder'])
    await page.keyboard.press('ArrowDown')
    await selected(['a.txt'])
    await page.keyboard.press('Shift+ArrowDown')
    await selected(['a.txt', 'b.txt'])
    await focused('b.txt')
    await page.keyboard.press('Shift+ArrowDown')
    await selected(['a.txt', 'b.txt', 'c.txt'])
    await page.keyboard.press('Shift+ArrowUp')
    await selected(['a.txt', 'b.txt'])
    await page.keyboard.press('Control+ArrowDown')
    await focused('c.txt')
    await selected(['a.txt', 'b.txt'])
    await page.keyboard.press('Control+Space')
    await selected(['a.txt', 'b.txt', 'c.txt'])
    await page.keyboard.press('Control+Space')
    await selected(['a.txt', 'b.txt'])
    await page.keyboard.press('Space')
    await selected(['c.txt'])
    await page.keyboard.press('Shift+Home')
    await selected(['folder', 'a.txt', 'b.txt', 'c.txt'])
    await page.keyboard.press('Control+End')
    await focused('more-39.txt')
    await selected(['folder', 'a.txt', 'b.txt', 'c.txt'])
    await expect
      .poll(() => pane.locator('.file-table').evaluate((el) => el.scrollTop))
      .toBeGreaterThan(0)
    await page.keyboard.press('Control+Space')
    await selected(['folder', 'a.txt', 'b.txt', 'c.txt', 'more-39.txt'])
    await page.keyboard.press('Control+Home')
    await page.keyboard.press('Control+Shift+ArrowDown')
    // Ctrl+Space established the last anchor at the end of the list.
    await expect(pane.locator('tbody tr.selected')).toHaveCount(44)
    await page.keyboard.press('Escape')
    await selected([])
    await page.keyboard.press('Control+a')
    await expect(pane.locator('tbody tr.selected')).toHaveCount(44)
    await expect(
      pane.getByRole('checkbox', { name: `${side} blocked 선택`, exact: true }),
    ).not.toBeChecked()
    await page.keyboard.press('Home')
    await page.keyboard.press('PageDown')
    const pageName = await pane
      .locator('tr.focused')
      .getAttribute('data-file-name')
    expect(pageName).not.toBe('folder')
    await selected([pageName])
    await page.keyboard.press('Shift+PageDown')
    expect(await pane.locator('tbody tr.selected').count()).toBeGreaterThan(1)
    await page.keyboard.press('Home')
    await page.keyboard.press('PageDown')
    await page.keyboard.press('PageUp')
    await focused('folder')
    await page.keyboard.press('End')
    await selected(['more-39.txt'])
    // Mouse selection provides the anchor for subsequent keyboard selection.
    await pane.getByRole('button', { name: '· a.txt', exact: true }).click()
    await page.keyboard.press('Shift+ArrowDown')
    await selected(['a.txt', 'b.txt'])
    await page.keyboard.press('b')
    await selected(['b.txt'])
    await page.keyboard.press('Home')
    await page.keyboard.press('Enter')
    await expect(pane.locator('tbody tr')).toHaveCount(0)
    await expect(grid).toBeFocused()
    await page.keyboard.press('Control+a')
    await selected([])
    await page.keyboard.press('ArrowDown')
    await selected([])
    await page.keyboard.press('Alt+ArrowUp')
    await expect(pane.locator('tbody tr')).toHaveCount(45)
    await page.keyboard.press('Home')
    await page.keyboard.press('Shift+End')
    await expect(pane.locator('tbody tr.selected')).toHaveCount(44)
    await page.keyboard.press('Home')
    await page.keyboard.press('Enter')
    await expect(pane.locator('tbody tr')).toHaveCount(0)
    await page.keyboard.press('Backspace')
    await expect(pane.locator('tbody tr')).toHaveCount(45)
    await page.keyboard.press('Home')
    await page.keyboard.press('Control+ArrowDown')
    await page.keyboard.press('Shift+Space')
    await selected(['folder', 'a.txt'])
    await page.keyboard.press('Escape')
    const path = pane.getByRole('textbox')
    await path.focus()
    await page.keyboard.press('Control+a')
    const selection = await path.evaluate((input) => [
      input.selectionStart,
      input.selectionEnd,
      input.value.length,
    ])
    expect(selection).toEqual([0, selection[2], selection[2]])
    await selected([])
    // Tab leaves the grid rather than walking through every file control.
    await grid.focus()
    await page.keyboard.press('Tab')
    await expect(
      pane.getByRole('checkbox', { name: `${side} 전체 선택`, exact: true }),
    ).toBeFocused()
    await page.keyboard.press('Tab')
    await expect(pane.locator('tbody :focus')).toHaveCount(0)
  }
})
