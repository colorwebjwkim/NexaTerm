import fs from 'node:fs/promises'
import { resolve, sep, posix } from 'node:path'
import ssh2 from 'ssh2'

export function serveSftp(session, root, behavior = {}) {
  session.on('sftp', (accept, reject) => {
    if (behavior.rejectSftp) return reject()
    const stream = accept()
    const handles = new Map()
    let next = 1
    const status = ssh2.utils.sftp.STATUS_CODE
    function path(name) {
      const normal = posix.normalize('/' + name)
      const result = resolve(root, '.' + normal)
      if (result !== root && !result.startsWith(root + sep))
        throw new Error('fixture path escape')
      return result
    }
    function handle(value) {
      const token = Buffer.alloc(4)
      token.writeUInt32BE(next++)
      handles.set(token.toString('hex'), value)
      return token
    }
    function get(token) {
      const value = handles.get(token.toString('hex'))
      if (!value) throw new Error('fixture handle')
      return value
    }
    function attrs(stat) {
      return {
        mode: stat.mode,
        uid: 0,
        gid: 0,
        size: stat.size,
        atime: Math.floor(stat.atimeMs / 1000),
        mtime: Math.floor(stat.mtimeMs / 1000),
      }
    }
    function on(event, work) {
      stream.on(event, (id, ...args) => {
        Promise.resolve()
          .then(() => work(id, ...args))
          .catch((error) => {
            if (error.code !== 'ENOENT')
              console.log(
                'SFTP_FIXTURE_FAIL',
                event,
                error.code || error.message,
              )
            stream.status(
              id,
              error.code === 'ENOENT'
                ? status.NO_SUCH_FILE
                : error.code === 'EACCES'
                  ? status.PERMISSION_DENIED
                  : status.FAILURE,
            )
          })
      })
    }
    on('REALPATH', async (id, name) =>
      stream.name(id, [
        {
          filename: posix.resolve('/', name),
          longname: '',
          attrs: attrs(await fs.stat(path(name))),
        },
      ]),
    )
    on('STAT', async (id, name) =>
      stream.attrs(id, attrs(await fs.stat(path(name)))),
    )
    on('LSTAT', async (id, name) =>
      stream.attrs(id, attrs(await fs.lstat(path(name)))),
    )
    on('OPENDIR', async (id, name) => {
      const entries = await fs.readdir(path(name))
      stream.handle(id, handle({ directory: name, entries, sent: false }))
    })
    on('READDIR', async (id, token) => {
      const directory = get(token)
      if (directory.sent) return stream.status(id, status.EOF)
      directory.sent = true
      if (!directory.entries.length) return stream.status(id, status.EOF)
      const entries = await Promise.all(
        directory.entries.map(async (name) => ({
          filename: name,
          longname: name,
          attrs: attrs(
            await fs.lstat(path(posix.join(directory.directory, name))),
          ),
        })),
      )
      stream.name(id, entries)
    })
    on('OPEN', async (id, name, flags, attributes) => {
      const nativeFlags =
        flags & 0x20
          ? flags & 1
            ? 'wx+'
            : 'wx'
          : ssh2.utils.sftp.flagsToString(flags)
      const file = await fs.open(
        path(name),
        nativeFlags,
        attributes.mode || 0o644,
      )
      stream.handle(id, handle({ file }))
    })
    on('READ', async (id, token, offset, length) => {
      const buffer = Buffer.alloc(Math.min(length, 262144))
      const { bytesRead } = await get(token).file.read(
        buffer,
        0,
        buffer.length,
        offset,
      )
      if (!bytesRead) stream.status(id, status.EOF)
      else stream.data(id, buffer.subarray(0, bytesRead))
    })
    on('WRITE', async (id, token, offset, data) => {
      let written = 0
      while (written < data.length) {
        const result = await get(token).file.write(
          data,
          written,
          data.length - written,
          offset + written,
        )
        written += result.bytesWritten
      }
      stream.status(id, status.OK)
    })
    on('FSTAT', async (id, token) =>
      stream.attrs(id, attrs(await get(token).file.stat())),
    )
    on('CLOSE', async (id, token) => {
      const value = get(token)
      handles.delete(token.toString('hex'))
      if (value.file) await value.file.close()
      stream.status(id, status.OK)
    })
    on('MKDIR', async (id, name) => {
      await fs.mkdir(path(name))
      stream.status(id, status.OK)
    })
    on('RMDIR', async (id, name) => {
      await fs.rmdir(path(name))
      stream.status(id, status.OK)
    })
    on('REMOVE', async (id, name) => {
      await fs.unlink(path(name))
      stream.status(id, status.OK)
    })
    on('RENAME', async (id, oldName, newName) => {
      try {
        await fs.lstat(path(newName))
        return stream.status(id, status.FAILURE)
      } catch (error) {
        if (error.code !== 'ENOENT') throw error
      }
      await fs.rename(path(oldName), path(newName))
      stream.status(id, status.OK)
    })
    on('SETSTAT', async (id, name, attributes) => {
      if (attributes.mode !== undefined)
        await fs.chmod(path(name), attributes.mode)
      stream.status(id, status.OK)
    })
    stream.on('close', () => {
      for (const value of handles.values()) value.file?.close().catch(() => {})
      handles.clear()
    })
  })
}
