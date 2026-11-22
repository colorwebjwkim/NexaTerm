import { invoke } from '@tauri-apps/api/core'
export interface FileEntry {
  name: string
  directory: boolean
  blocked: boolean
  size: number
  modified: number
  mode: number | null
}
export interface Listing {
  path: string
  entries: FileEntry[]
}
export interface LocalRoot {
  token: string
  path: string
}
export interface Transfer {
  id: string
  connectionId: string
  direction: string
  name: string
  status: string
  bytes: number
  total: number
  files: number
  skipped: number
  speed: number
  message: string
  conflict: string | null
}
export interface TransferSpec {
  connectionId: string
  direction: string
  localRoot: string
  localDir: string
  remoteDir: string
  names: string[]
  collision: string
}
export const activeTransfer = (job: Transfer) =>
  ['queued', 'scanning', 'running', 'waiting'].includes(job.status)
export const files = {
  roots: () => invoke<LocalRoot[]>('local_roots'),
  openLocal: () =>
    invoke<{ root: LocalRoot; listing: Listing }>('open_local_location'),
  chooseRoot: () => invoke<LocalRoot | null>('choose_local_root'),
  local: (root: string, path: string, remember = false) =>
    invoke<Listing>('local_list', { root, path, remember }),
  remote: (connectionId: string, path: string) =>
    invoke<Listing>('remote_list', { connectionId, path }),
  jobs: () => invoke<Transfer[]>('transfer_queue'),
  enqueue: (spec: TransferSpec) => invoke<string>('enqueue_transfer', { spec }),
  cancel: (id: string) => invoke<void>('cancel_transfer', { id }),
  retry: (id: string) => invoke<string>('retry_transfer', { id }),
  decide: (id: string, decision: string) =>
    invoke<void>('resolve_transfer_conflict', { id, decision }),
  reset: (connectionId: string) => invoke<void>('reset_sftp', { connectionId }),
  operation: (
    connectionId: string,
    side: string,
    root: string,
    directory: string,
    name: string,
    operation: string,
    newName = '',
    mode: number | null = null,
    confirmed = false,
  ) =>
    invoke<void>('file_operation', {
      connectionId,
      side,
      root,
      directory,
      name,
      operation,
      newName,
      mode,
      confirmed,
    }),
}
export function size(value: number): string {
  if (value < 1024) return `${value} B`
  const units = ['KiB', 'MiB', 'GiB', 'TiB']
  let index = -1
  do {
    value /= 1024
    index++
  } while (value >= 1024 && index < units.length - 1)
  return `${value.toFixed(1)} ${units[index]}`
}
