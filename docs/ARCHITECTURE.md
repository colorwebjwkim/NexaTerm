# NexaTerm 0.3 아키텍처

현재 제품 소개와 실행 안내는 [README.md](../README.md), 요구사항과 개발 이력은 [PROJECT.md](../PROJECT.md)를 참고한다. 상용화 시 핵심 코드 보호를 고려해 Electron Main/Preload를 제거하고 Tauri 2 + Rust로 전환했다. UI는 Svelte 5와 xterm.js를 사용한다. 실행 가능한 1차 버전이며 전체 MVP가 완성된 상태는 아니다.

## 소유권과 경계

- `src-tauri/src/models.rs`: 프로필·설정·저장 상태의 스키마와 상한 검증.
- `store.rs`: SQLite 단일 상태 트랜잭션. 프로필은 secretRef만 보유하고 별도 secrets 맵에는 DPAPI 암호문만 저장한다.
- `secrets.rs`: Windows 사용자 범위 DPAPI 보호/복호화. 다른 OS에서는 비밀정보 저장을 거부한다.
- `backup.rs`: 버전이 있는 암호화 포맷, 파일 입출력, 인증 검증, 병합/교체 복원.
- `ssh.rs`: 연결마다 독립된 russh 클라이언트, PTY, 이벤트 채널과 입력 큐. 호스트키 확인 및 keepalive/타임아웃.
- `commands.rs`: 허용된 IPC 명령과 호출자 검증, 네이티브 파일 선택, 복원 미리보기 수명.
- `src/renderer/api.ts`: 좁은 IPC 계약. 복호화된 저장 비밀번호를 조회하는 API는 없다.
- `App.svelte` / `TerminalView.svelte`: 프로필·백업 UI, 독립 탭과 xterm 렌더링.

프로필 ID, 연결 ID, 탭 ID는 별도로 생성한다. 탭 전환 시 터미널 컴포넌트를 파괴하지 않아 버퍼와 연결이 유지된다. 32개 동시 연결, 입력 큐 64개, 출력 미처리 약 128KiB 상한을 둔다. xterm write 완료 후 sequence ACK를 보내 출력 생산 속도를 제한한다. 설정 가능한 스크롤 버퍼도 제한한다.

SQLite에는 DPAPI 암호문까지 한 트랜잭션으로 저장하므로 복원 시 별도 OS Vault와의 분산 트랜잭션이 필요하지 않다. 복원 연결 차단과 새 연결 시작은 같은 operation 잠금으로 직렬화한다. 저장 상태 접근은 Mutex와 DB 트랜잭션으로 직렬화하고 디스크·백업 암호화 작업은 blocking 작업 풀에서 처리한다.

## SFTP 및 전송 수명

`sftp.rs`의 FileManager는 연결별 SFTP 세션, 사용자가 선택한 로컬 폴더 접근 토큰, 전송 작업/취소/충돌 대기 상태를 소유한다. `file_commands.rs`가 IPC와 네이티브 로컬 폴더 선택을 담당하고 SftpPane/TransferQueue가 표시한다. 파일 본문은 WebView를 통과하지 않는다. Rust가 128KiB 단위로 읽고 쓰며 SFTP 라이브러리의 유한한 요청 파이프라인을 사용한다. 동시 작업은 2개, 대기 작업 100개, 표시 이력 300개로 제한한다. 큐/이력은 현재 앱 실행 동안만 유지한다.

SSH 인증 완료 후 Handle을 Arc로 공유하여 shell과 SFTP 채널을 같은 SSH 연결에서 연다. SFTP 초기화/작업 오류는 터미널을 자동 종료하지 않는다. 탭 종료 시 진행 중 작업이 있으면 전송 유지/전송 취소 후 닫기/닫기 취소를 선택한다. 유지 시 shell 채널만 닫고 Rust 연결 루프는 남으며 마지막 작업이 끝나면 연결을 정리한다. 앱 자체를 종료하면 백그라운드 전송도 종료된다.

로컬 폴더 토큰은 앱 수명에 한정되고 백업에 포함하지 않는다. 모든 로컬 경로는 해당 폴더 기준 상대 경로로 검증하며 링크/정션을 차단한다. 원격 폴더는 서버 canonicalize 결과를 사용한다. 재귀 전송은 최대 깊이 64/항목 10,000개이며 링크를 따라가지 않는다. Telnet은 아직 미구현이며 추가 시 별도 IAC 상태 기계를 사용하고 SFTP를 노출하지 않는다.
