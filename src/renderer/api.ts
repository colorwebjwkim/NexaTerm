import { Channel, invoke } from '@tauri-apps/api/core'
export interface Profile {
  id: string
  name: string
  host: string
  port: number
  username: string
  group: string
  tags: string[]
  favorite: boolean
  authMethod: 'password' | 'privateKey'
  keyPath: string
  secretRef: string | null
  connectionType: 'ssh' | 'sftp'
  hostKeyAlgorithm: string
}
export interface Settings {
  theme: 'system' | 'dark' | 'light'
  fontSize: number
  scrollback: number
  confirmExit: boolean
}
export interface AppView {
  profiles: Profile[]
  settings: Settings
}
export interface Preview {
  token: string
  profiles: string[]
  passwordCount: number
  hasSettings: boolean
}
export type SessionEvent =
  | { kind: 'authenticationRejected' }
  | { kind: 'state'; state: string; message: string }
  | { kind: 'data'; data: number[]; sequence: number }
  | {
      kind: 'hostKey'
      token: string
      host: string
      algorithm: string
      fingerprint: string
      previous: string | null
    }
export const api = {
  state: () => invoke<AppView>('get_state'),
  save: (profile: Profile, password: string, saveSecret: boolean) =>
    invoke<AppView>('save_profile', {
      profile,
      password: password || null,
      saveSecret,
    }),
  remove: (id: string) => invoke<AppView>('delete_profile', { id }),
  settings: (settings: Settings) =>
    invoke<AppView>('save_settings', { settings }),
  keyFile: () => invoke<string | null>('choose_private_key'),
  backup: (password: string, includeSecrets: boolean) =>
    invoke<string | null>('export_backup', { password, includeSecrets }),
  preview: (password: string) =>
    invoke<Preview | null>('preview_backup', { password }),
  restore: (token: string, mode: string, confirmReplace: boolean) =>
    invoke<AppView>('restore_backup', { token, mode, confirmReplace }),
  cancelRestore: () => invoke<void>('cancel_restore'),
  connect: (
    profileId: string,
    password: string,
    handler: (event: SessionEvent) => void,
  ) => {
    const events = new Channel<SessionEvent>()
    events.onmessage = handler
    return invoke<string>('connect_session', {
      profileId,
      password: password || null,
      events,
    })
  },
  input: (connectionId: string, data: string) =>
    invoke<void>('terminal_input', { connectionId, data }),
  resize: (connectionId: string, cols: number, rows: number) =>
    invoke<void>('terminal_resize', { connectionId, cols, rows }),
  disconnect: (connectionId: string, transferPolicy?: string) =>
    invoke<void>('disconnect_session', {
      connectionId,
      transferPolicy: transferPolicy || null,
    }),
  ack: (connectionId: string, sequence: number) =>
    invoke<void>('terminal_ack', { connectionId, sequence }),
  trust: (token: string, approve: boolean, approveChanged: boolean) =>
    invoke<void>('resolve_host_key', { token, approve, approveChanged }),
}
