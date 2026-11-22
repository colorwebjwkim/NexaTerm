# NexaTerm 0.3.3 검증 — 2026-10-30

- Rust 테스트: 14개 성공. 기존 프로필 기본값, 알고리즘 입력 검증/영속성, 명시적 알고리즘 불일치 및 재시도 포함.
- cargo clippy --all-targets -- -D warnings, cargo fmt -- --check, pnpm lint, pnpm typecheck, Vite release build 통과.
- Windows WebView2 E2E: ssh2 독립 서버에서 호스트키를 ssh-rsa만 제공하도록 제한. 최초 신뢰 거부/승인, 비밀번호 DPAPI 저장/재실행, 저장·미저장 비밀번호를 사용하는 같은 탭 Enter 재접속, EOF 후 재접속, 강제 알고리즘 불일치 오류 후 설정 수정/Enter 재시도, 3개 탭 Ctrl+Tab/Ctrl+Shift+Tab 방향 및 순환 통과.
- SFTP 회귀: 300개 파일 왕복과 해시, 충돌 정책, 취소/재시도, 경로 이탈 차단, SFTP 전용 연결, 터미널 크기/스크롤/분할 회귀 통과.
- 최종 빌드: pnpm exec tauri build --bundles msi --ci 성공. MSI: src-tauri/target/release/bundle/msi/NexaTerm_0.3.3_x64_en-US.msi.
- 일반 권한 설치는 기존 0.3.1 시스템 전체 설치 제거 중 오류 1730/종료 코드 1603. 관리자 권한 재시도에서 종료 코드 0. 설치본 ProductVersion/FileVersion 0.3.3 확인.
- 설치본 smoke: native IPC, Node API 미노출, 메뉴, 서버 목록 숨김/복원, 버전 정보, 호스트키 알고리즘 기본값 및 8개 옵션, Ctrl+Shift+Tab 도움말 통과. 설치본을 재실행하여 프로세스와 화면 확인.
- 설치 EXE와 release EXE의 차이는 Tauri의 번들 식별 문자열 MSI/UNK 3바이트뿐임을 확인.

## 남은 확인 사항

- 실제 211.61.141.28:22는 이 환경의 TCP 연결이 ETIMEDOUT으로 종료되어 SSH 협상/로그인은 검증하지 못했다.
- 설치 전 실행 화면에서 프로필 4개, 설치 후 재실행 화면에서는 192.168.100.3 프로필 1개가 표시됐다. 확인한 Roaming 및 Codex 패키지 캐시의 settings.db는 프로필 1개이며 파일 수정일은 2026-10-26이다. 설치/검증 과정은 해당 DB를 덮어쓰지 않았다. 이전 실행 화면과 디스크 상태의 차이는 원인을 확정하지 못했으며 기존 호스트·계정·자격 증명을 추정해 복원하지 않았다.
- RSA 개인키 인증은 보안 제한으로 거부한다. RSA 서버 공개 호스트키 검증만 활성화한 근거와 의존성 취약점은 SECURITY.md 참조.
- cargo-audit는 미설치로 실행하지 못했다. 코드 서명, 앱 제거는 검증하지 않았다.

## 2026-10-30 서버 목록 복구

- 현재 DB를 `reports/profile-recovery-20261030/settings-before-recovery-*.db` 및 `live-before-restore-*.db`에 보관했다.
- C: 볼륨의 2026-10-30 15:29:17 Windows 스냅샷(HarddiskVolumeShadowCopy9)에서 원래 Roaming DB의 프로필 4개와 DPAPI 암호문 4개, 신뢰 키 3개를 확보했다. 같은 스냅샷의 Codex 패키지 캐시 DB에는 프로필 1개만 있었다. 스냅샷을 수정하지 않고 파일 복사본에서 검증했다.
- DPAPI 암호문 4개 모두 현재 Windows 사용자로 복호화 가능한지 확인했다. 비밀번호 평문은 로그·파일에 기록하지 않고 복호화 결과 버퍼를 지웠다.
- 원본 프로필 ID·서버 주소·계정·DPAPI 암호문을 유지하여 복구했다. 현재 앱 설정과 충돌 없는 호스트키 신뢰를 보존했다. Roaming 및 Codex 패키지 캐시 DB를 SQLite 트랜잭션으로 갱신하고 integrity_check=ok를 확인했다.
- 설치된 0.3.3 앱을 재실행한 후 실제 WebView IPC get_state와 서버 목록 버튼 4개를 검사했다. 서버 주소 4개 및 저장 비밀번호 참조 4개, ssh-rsa 기본값을 확인했다.
- 복구 대상은 192.168.100.3, 211.61.141.28, 211.61.141.10, NAS(192.168.1.250)이다. 실제 원격 로그인 검증은 이 복구 검사에 포함하지 않았다.
- 재시작 전 DB를 확보하지 못했던 절차상의 문제를 기록한다. 원래 저장 위치와 Codex 패키지 캐시의 차이는 확인됐으나 현재 DB가 이전 상태로 보였던 전체 경위는 단정하지 않는다.
