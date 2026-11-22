import type { Profile } from './api'

export type ProfileField =
  | 'name'
  | 'host'
  | 'port'
  | 'username'
  | 'group'
  | 'tags'
  | 'keyPath'
  | 'password'

// Rust validates UTF-8 byte lengths, including for Korean profile names.
export function validateProfile(
  profile: Profile,
  tagText: string,
  password: string,
  saveSecret: boolean,
  savedProfile?: Profile,
): Partial<Record<ProfileField, string>> {
  const errors: Partial<Record<ProfileField, string>> = {}
  const bytes = (text: string) => new TextEncoder().encode(text).length
  if (!profile.name.trim()) errors.name = '프로필 이름을 입력하세요.'
  else if (bytes(profile.name) > 160)
    errors.name = '프로필 이름은 UTF-8 기준 160바이트 이내로 입력하세요.'
  if (!profile.host.trim()) errors.host = '호스트를 입력하세요.'
  else if (bytes(profile.host) > 253 || /[\s\p{Cc}/\\]/u.test(profile.host))
    errors.host =
      '호스트는 253바이트 이내의 IP 또는 호스트명으로 입력하세요. 공백, 경로, 제어 문자는 사용할 수 없습니다.'
  if (
    !Number.isInteger(profile.port) ||
    profile.port < 1 ||
    profile.port > 65535
  )
    errors.port = '포트는 1~65535 사이의 정수로 입력하세요.'
  if (!profile.username.trim()) errors.username = '사용자명을 입력하세요.'
  else if (bytes(profile.username) > 128 || /\p{Cc}/u.test(profile.username))
    errors.username = '사용자명은 제어 문자 없이 128바이트 이내로 입력하세요.'
  if (bytes(profile.group) > 160)
    errors.group = '그룹은 160바이트 이내로 입력하세요.'
  const tags = tagText
    .split(',')
    .map((tag) => tag.trim())
    .filter(Boolean)
  if (tags.length > 32 || tags.some((tag) => bytes(tag) > 80))
    errors.tags = '태그는 최대 32개, 각 80바이트 이내로 입력하세요.'
  if (profile.authMethod === 'privateKey' && !profile.keyPath.trim())
    errors.keyPath = '개인키 파일 경로를 입력하세요.'
  else if (bytes(profile.keyPath) > 4096)
    errors.keyPath = '개인키 경로는 4096바이트 이내로 입력하세요.'
  if (
    saveSecret &&
    !password &&
    !(savedProfile?.secretRef && savedProfile.authMethod === profile.authMethod)
  )
    errors.password = '저장할 비밀번호 또는 passphrase를 입력하세요.'
  return errors
}
