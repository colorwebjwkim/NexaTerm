# 개발 및 실행

## 실행 파일

`src-tauri/target/release/nexaterm.exe`는 직접 실행하는 Windows x64 앱이다. 설치 파일 생성 명령은 `pnpm dist:win`이며 NSIS 결과는 `src-tauri/target/release/bundle/nsis`에 생성된다. 배포물은 아직 코드 서명이 없는 개발/검토용이다.

현재 PC에는 WebView2가 설치되어 있다. 다른 PC의 직접 실행에는 WebView2가 필요하고, NSIS 설치기는 필요 시 WebView2 bootstrapper를 실행하므로 인터넷 접근이 필요할 수 있다. 오프라인 설치와 기업 정책 환경은 별도 검증 대상이다.

## 개발 환경

Windows 11 x64, Node.js 24+, pnpm 11.25.0, Rust 1.99.0 MSVC, Visual Studio C++ Build Tools와 Windows SDK, WebView2가 필요하다. 이번 작업에서 Rust를 설치했다. 기존 터미널에서 cargo가 인식되지 않으면 터미널을 다시 연다.

```powershell
pnpm install --frozen-lockfile
pnpm dev
```

```powershell
pnpm lint
pnpm format:check
pnpm typecheck
pnpm test
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm build
pnpm start
pnpm test:release
pnpm dist:win
```

실제 Windows WebView E2E는 아래 순서로 실행한다. 테스트는 임시 데이터 디렉터리, 로컬 전용 SSH 서버와 디버깅 포트를 사용한다. 실제 사용자 비밀번호나 서버는 사용하지 않는다. 테스트가 생성한 파일에는 합성 자격 증명만 들어간다.

```powershell
pnpm exec tauri build --debug --no-bundle
pnpm test:e2e
```

`NEXATERM_TEST_DATA_DIR`는 debug 빌드에서만 사용한다. release 빌드는 이 환경변수를 무시한다. E2E 스크린샷은 `test-results/desktop-workspace.png`, `desktop-ssh.png`이다.

## 사용 흐름

1. 서버 프로필 생성 → 호스트/포트/계정/비밀번호 또는 개인키 경로 입력 → 필요하면 비밀번호 저장 체크 → 저장.
   `접속 방식`은 SSH 또는 SFTP 전용이다. SFTP 전용은 셸과 PTY를 요청하지 않고 파일 관리자만 전체 작업 영역에 연다. 비밀번호/개인키 인증, 호스트키 확인, 전송 큐와 마지막 로컬 위치는 공통으로 사용한다. 기존 프로필과 기존 백업에 접속 방식이 없으면 SSH로 처리한다.
2. 왼쪽 프로필 클릭 → 저장하지 않은 인증 정보 입력 → 필요하면 `이 PC에 비밀번호 저장` 선택 → 최초 호스트키 지문 확인 → 승인 → SSH 셸. 저장 선택은 기본 해제이며, 선택하면 연결 시도 전에 DPAPI로 저장해 다음 접속에 사용한다. 같은 프로필을 다시 클릭하면 별도 연결 탭이 열린다.
   콘솔 위에서 `Ctrl + 마우스 휠 위/아래` 또는 하단 상태 표시줄의 `돋보기 +/−` 버튼으로 글꼴 크기를 조절한다. 10~28px 범위에서 모든 콘솔 탭에 즉시 반영되며 다음 실행에도 유지된다. 일반 휠은 기존 스크롤로 동작한다.

3. 백업 → 비밀번호 포함 여부 선택 → 별도 12자 이상 백업 암호 두 번 입력 → `.nxtbackup` 파일 저장.
4. 모든 연결 종료 → 복원 → 백업 암호 입력/파일 선택 → 프로필 목록·비밀번호 개수 미리보기 → 병합 또는 전체 교체 확인. 병합은 기존 앱 설정을 유지한다.
5. 설정에서 테마·터미널 글자 크기·스크롤 버퍼를 저장할 수 있다.
6. SSH 연결 후 `파일 열기 (SFTP)` 또는 F9 → 로컬/원격 파일이나 폴더를 체크 → 업로드/다운로드. 로컬은 처음에 Windows 문서 폴더로 열리고 이후 마지막으로 이동한 위치를 앱 재시작 후에도 사용한다. 문서 폴더는 OneDrive 등 Windows의 경로 재지정을 따른다. 저장 위치가 없어지거나 접근할 수 없으면 문서 폴더(접근 불가 시 사용자 홈)로 돌아간다. `폴더 선택`으로 다른 기준 폴더를 지정할 수 있다. 이 PC의 로컬 위치는 설정 백업·복원 대상에서 제외한다. 상위 폴더·경로 입력·숨김 파일 표시·새로고침을 지원한다.
   SSH와 SFTP 사이 구분선을 위아래로 드래그해 높이를 조절한다. 두 번 클릭하면 반반으로 돌아가며, 구분선에 키보드 초점을 두고 위/아래 방향키로도 조절할 수 있다. 분할 비율은 현재 탭에서 SFTP를 닫았다 열어도 유지된다. SSH, SFTP 파일 목록과 전송 큐는 각각 내부에서 스크롤하며 전체 작업 영역은 스크롤되지 않는다.
7. 동일 이름은 기본 매번 묻기이며 큐에서 건너뛰기/덮어쓰기/이름 변경을 선택한다. 폴더는 병합하고 파일 충돌에 정책을 적용한다. 전송 중에는 큐에서 취소할 수 있으며 실패/취소 작업의 재전송은 연결이 살아 있는 동안 가능하다.
8. 이름 변경·폴더 생성·파일/빈 폴더 삭제, 원격 권한 000~777 변경을 제공한다. 삭제는 별도 체크 후 적용한다. 비어 있지 않은 폴더 삭제는 거부한다. SFTP 오류 시 터미널을 유지한 채 `SFTP 재연결`로 재시도할 수 있다.

스트레스 테스트는 `NEXATERM_SFTP_STRESS=1` 환경변수를 설정하고 `pnpm test:e2e`를 실행한다. 실제 1GiB 파일 왕복/중간 취소/백그라운드 전송을 포함해 임시 디스크 공간 수 GiB를 사용한다. Debug 테스트의 `NEXATERM_TEST_LOCAL_ROOT`는 네이티브 폴더 선택과 동일한 검증 함수로 테스트 폴더를 등록하며 release는 이를 무시한다. WebView 테스트 캐시는 별도 임시 폴더에 격리한다.

Windows 앱 데이터 경로는 `%APPDATA%\com.nexaterm.desktop\settings.db`이다. DB 파일 단독 복사는 다른 PC로 옮기는 자격 증명 백업 방법이 아니다.

## 의존성 선택 (2026-10-14)

- Tauri Rust/API 2.12.3, CLI 2.12.2, tauri-build 2.7.2: 안정 2.x 계열. 3.x alpha는 선택하지 않았다.
- Svelte 5.57.2, Vite 7.3.7, Svelte 플러그인 6.2.4, TypeScript 6.0.3: 기존 검증된 호환 조합 유지. Tauri로 전환해도 불필요한 메이저 업데이트를 함께 하지 않았다.
- xterm 6.0.0 / fit 0.11.0, russh 0.64.1, rusqlite 0.40.2, rfd 0.17.2, windows-sys 0.61.2를 조회·채택했다.
- 백업에는 호환 API가 검증된 aes-gcm 0.10.3 / argon2 0.5.3 조합을 사용한다. 최신 조회 버전은 0.11.1 / 0.6.0이며 russh 간접 의존성에는 새 버전도 포함된다. 보안 감사 결과를 별도 확인했다.
- 초기에는 russh 기본 기능을 끄고 ring만 활성화했다. 0.3.3에서는 ring + rsa로 서버 RSA 호스트키 검증을 지원한다. RSA 개인키 인증은 명시적으로 거부한다. 상세 근거는 SECURITY.md의 2026-10-30 기록을 따른다.
- ssh2 1.17.0은 독립 구현과의 상호운용 E2E용 개발 의존성으로만 사용한다. 제품 실행에는 Node.js/ssh2/Electron이 필요 없다. ssh2의 선택적 native 최적화 빌드는 비활성화했다.
- pnpm-lock.yaml / Cargo.lock을 커밋 대상에 포함하며 바이너리·앱 데이터·비밀번호·테스트 결과는 제외한다.
- SFTP는 Apache-2.0 russh-sftp 3.0.1을 기반으로 로컬 패치한다. 바이너리 핸들의 손실 UTF-8 변환을 수정하고 폴더 목록/오류 정리를 제한했다. 상세 변경 및 유지보수 기준은 `src-tauri/vendor/russh-sftp/NEXATERM_PATCH.md`에 기록한다.

공식 근거: [Tauri prerequisites](https://tauri.app/start/prerequisites/), [Tauri security](https://tauri.app/security/), [Tauri channels](https://tauri.app/develop/calling-frontend/), [russh](https://github.com/Eugeny/russh).
