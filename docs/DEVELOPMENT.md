# 개발 및 실행

## 실행 파일

2026-11-23 이 PC의 설치본에 SFTP 탐색기 방식 마우스·키보드 선택(행 클릭·영역 드래그·방향키·Ctrl/Shift·전체 선택)을 추가 적용했다. 일반 UI 드래그 선택 방지와 터미널 외 오른쪽 클릭 메뉴 제한도 포함한다. release EXE를 직접 교체했으며 설치본 WebView의 시작·UI 검사와 설정 DB 해시 보존을 확인했다. 설치 패키지는 이번 작업에서 재생성하지 않았다. 상세 백업·검증 범위는 [프로젝트 기록](../PROJECT.md)을 참고한다.

`src-tauri/target/release/nexaterm.exe`는 직접 실행하는 Windows x64 앱이다. 설치 파일 생성 명령은 `pnpm dist:win`이며 NSIS 결과는 `src-tauri/target/release/bundle/nsis`에 생성된다. 배포물은 아직 코드 서명이 없는 개발/검토용이다.

현재 PC에는 WebView2가 설치되어 있다. 다른 PC의 직접 실행에는 WebView2가 필요하고, NSIS 설치기는 필요 시 WebView2 bootstrapper를 실행하므로 인터넷 접근이 필요할 수 있다. 오프라인 설치와 기업 정책 환경은 별도 검증 대상이다.

## 개발 환경

아래 도구는 소스를 빌드할 때만 필요하다. 완성된 실행 파일을 사용하는 PC에는 Node.js·npm·pnpm·Rust·Cargo·Visual Studio Build Tools·Windows SDK·Git을 설치하지 않아도 된다.

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

전역 `contextmenu` 이벤트에서 `.xterm` 내부만 예외로 두고 나머지 영역은 기본 메뉴를 차단한다. 메뉴·서버 목록·SFTP·입력창·대화상자에 공통 적용하며 터미널의 오른쪽 클릭 처리와 키보드 복사·붙여넣기는 유지한다. 컴포넌트 해제 시 이벤트 리스너를 제거한다.

일반 UI는 드래그 시 브라우저처럼 텍스트가 선택되지 않도록 한다. 전역 `body`에 `user-select: none`을 적용하고 입력창·편집 영역·`pre`·`code`·오류 메시지는 `text`로 허용한다. xterm의 자체 선택 처리는 유지하므로 터미널 출력은 계속 드래그로 선택·복사할 수 있다. 탭 순서 변경과 SSH/SFTP 구분선 드래그는 기존 동작을 사용한다.

터미널 안에서도 `Alt+F4`로 앱을 종료할 수 있다. 활성 연결이 있고 종료 확인 설정이 켜져 있으면 기존 확인창을 표시하며, 취소하면 연결을 유지한다. 이 키는 원격 서버로 전달하지 않는다.

2026-11-23 로컬 SSH 검증에서 종료 확인을 끈 상태의 Alt+F4 종료 후 서버 측 연결 수 0을 확인했다. E2E는 테스트의 연결 강제 정리 전에 이를 확인한다. 이후 재실행한 무연결 앱의 키 입력 검사에는 미해결 실패가 있으므로 전체 실행 결과와 제한은 [프로젝트 기록](../PROJECT.md)을 참고한다.

1. 서버 프로필 생성 → 호스트/포트/계정/비밀번호 또는 개인키 경로 입력 → 필요하면 비밀번호 저장 체크 → 저장.
   `접속 방식`은 SSH 또는 SFTP 전용이다. SFTP 전용은 셸과 PTY를 요청하지 않고 파일 관리자만 전체 작업 영역에 연다. 비밀번호/개인키 인증, 호스트키 확인, 전송 큐와 마지막 로컬 위치는 공통으로 사용한다. 기존 프로필과 기존 백업에 접속 방식이 없으면 SSH로 처리한다.
2. 왼쪽 프로필 클릭 → 저장하지 않은 인증 정보 입력 → 필요하면 `이 PC에 비밀번호 저장` 선택 → 최초 호스트키 지문 확인 → 승인 → SSH 셸. 저장 선택은 기본 해제이며, 선택하면 연결 시도 전에 DPAPI로 저장해 다음 접속에 사용한다. 같은 프로필을 다시 클릭하면 별도 연결 탭이 열린다.
   콘솔 위에서 `Ctrl + 마우스 휠 위/아래` 또는 하단 상태 표시줄의 `돋보기 +/−` 버튼으로 글꼴 크기를 조절한다. 10~28px 범위에서 모든 콘솔 탭에 즉시 반영되며 다음 실행에도 유지된다. 일반 휠은 기존 스크롤로 동작한다.

3. 백업 → 비밀번호 포함 여부 선택 → 별도 12자 이상 백업 암호 두 번 입력 → `.nxtbackup` 파일 저장.
4. 모든 연결 종료 → 복원 → 백업 암호 입력/파일 선택 → 프로필 목록·비밀번호 개수 미리보기 → 병합 또는 전체 교체 확인. 병합은 기존 앱 설정을 유지한다.
5. `도구 → 환경 설정`에서 테마·터미널 글자 크기·스크롤 버퍼를 저장할 수 있다. 테마는 `시스템 설정 따르기 (Windows)`(새 설정의 기본값), `다크`, `라이트` 중 선택한다. `보기 → Windows 테마 따르기`로 자동 모드를 바로 켤 수 있다. 시스템 모드는 Windows 앱 모드 변경을 실행 중 화면·열린 터미널·제목 표시줄에 반영하며 수동 모드는 고정한다. 저장 후 재실행 및 설정 전체 교체 복원에도 선택을 유지한다. 설정 창에서 취소하면 변경을 적용하지 않는다.
6. SSH 연결 후 `파일 열기 (SFTP)` 또는 F9 → 로컬/원격 파일이나 폴더를 선택 → 업로드/다운로드. 로컬은 처음에 Windows 문서 폴더로 열리고 이후 마지막으로 이동한 위치를 앱 재시작 후에도 사용한다. 문서 폴더는 OneDrive 등 Windows의 경로 재지정을 따른다. 저장 위치가 없어지거나 접근할 수 없으면 문서 폴더(접근 불가 시 사용자 홈)로 돌아간다. `폴더 선택`으로 다른 기준 폴더를 지정할 수 있다. 이 PC의 로컬 위치는 설정 백업·복원 대상에서 제외한다. 상위 폴더·경로 입력·숨김 파일 표시·새로고침을 지원한다.
   양쪽 목록에서 행 클릭은 단일 선택, Ctrl+클릭은 개별 선택/해제, Shift+클릭은 마지막 기준 항목부터 표시 순서의 범위 선택, Ctrl+Shift+클릭은 범위 추가다. 체크박스는 개별 선택을 토글한다. 폴더는 더블클릭 또는 목록 초점에서 Enter로 연다. Space는 초점 항목 선택, Ctrl+Space는 선택 토글, Shift+Space는 범위 선택이다. 영역 드래그는 여러 행을 선택하고 가장자리에서는 자동 스크롤한다. Ctrl+드래그는 영역 내 선택을 반전하고 Shift+드래그는 기존 선택에 추가한다. 빈 공간 클릭은 해당 패널의 선택을 해제한다. 선택 불가 항목과 숨겨진 항목은 범위·드래그의 새 선택 대상에서 제외하며 폴더 이동 시 선택과 기준 항목을 초기화한다. 드래그 앤 드롭 전송은 지원하지 않는다.
   파일 목록은 Tab으로 진입하며 행별 체크박스·이름 버튼을 모두 Tab으로 순회하지 않는다. ↑/↓·Home/End·Page Up/Down은 이동 및 단일 선택, Shift+이동은 범위 확장/축소, Ctrl+이동은 선택을 유지한 초점 이동, Ctrl+Shift+이동은 범위 추가다. Ctrl+A는 표시된 선택 가능 항목 전체 선택, Esc는 선택 해제다. 초점 행은 선택 강조와 별도의 테두리로 표시하고 화면 밖으로 이동하면 내부 목록을 스크롤한다. 이름 문자 입력은 접두어 탐색(1초 내 입력 연결, 같은 문자 반복은 다음 일치 항목)이다. Enter는 폴더 열기, Alt+↑/Backspace는 상위 폴더 이동이다. 입력창·헤더 전체 선택 체크박스·다른 패널의 기본 키 동작은 유지한다.
   SSH와 SFTP 사이 구분선을 위아래로 드래그해 높이를 조절한다. 두 번 클릭하면 반반으로 돌아가며, 구분선에 키보드 초점을 두고 위/아래 방향키로도 조절할 수 있다. 분할 비율은 현재 탭에서 SFTP를 닫았다 열어도 유지된다. SSH, SFTP 파일 목록과 전송 큐는 각각 내부에서 스크롤하며 전체 작업 영역은 스크롤되지 않는다.
7. 동일 이름은 기본 매번 묻기이며 큐에서 건너뛰기/덮어쓰기/이름 변경을 선택한다. 폴더는 병합하고 파일 충돌에 정책을 적용한다. 전송 중에는 큐에서 취소할 수 있으며 실패/취소 작업의 재전송은 연결이 살아 있는 동안 가능하다.
8. 이름 변경·폴더 생성·파일/빈 폴더 삭제, 원격 권한 000~777 변경을 제공한다. 삭제는 별도 체크 후 적용한다. 비어 있지 않은 폴더 삭제는 거부한다. SFTP 오류 시 터미널을 유지한 채 `SFTP 재연결`로 재시도할 수 있다.

스트레스 테스트는 `NEXATERM_SFTP_STRESS=1` 환경변수를 설정하고 `pnpm test:e2e`를 실행한다. 실제 1GiB 파일 왕복/중간 취소/백그라운드 전송을 포함해 임시 디스크 공간 수 GiB를 사용한다. Debug 테스트의 `NEXATERM_TEST_LOCAL_ROOT`는 네이티브 폴더 선택과 동일한 검증 함수로 테스트 폴더를 등록하며 release는 이를 무시한다. WebView 테스트 캐시는 별도 임시 폴더에 격리한다.

Windows 앱 데이터 경로는 `%APPDATA%\com.nexaterm.desktop\settings.db`이다. DB 파일 단독 복사는 다른 PC로 옮기는 자격 증명 백업 방법이 아니다.

## SFTP 선택 UI 회귀 검사

`pnpm exec playwright test tests/e2e/sftp-selection.spec.mjs`는 Vite에서 실제 SFTP 컴포넌트를 로드하고 파일 IPC만 합성 목록으로 대체한다. Chrome이 설치된 환경에서 양쪽 패널의 행·Ctrl·Shift·Ctrl+Shift·체크박스·영역 드래그·자동 스크롤·빈 공간 해제·폴더 더블클릭을 검증한다. 키보드 회귀는 양쪽 패널의 범위 확장/축소, Ctrl 초점 이동·Space 토글, 전체 선택·해제, 페이지 이동·자동 스크롤, 마우스 기준 연계, 이름 탐색, 빈 폴더 Enter·상위 이동과 Tab 순서를 검증한다. 실제 파일 전송·Windows WebView 통합 검사는 기존 desktop E2E와 별도다.

## 개발 도구 없는 실행 환경 검증 (2026-11-22)

- 설치본 release EXE와 현재 debug EXE의 PE DLL 의존성을 확인했다. Windows 시스템 DLL·Universal CRT API 의존성만 확인했으며 `node.dll`, `VCRUNTIME*.dll`, `MSVCP*.dll`이나 별도 OpenSSL·SQLite DLL 의존성은 없었다. 현재 확인한 EXE에 별도 Visual C++ Redistributable 설치가 필요하다는 근거는 발견되지 않았다. 배포 바이너리가 달라지면 다시 확인한다.
- 두 EXE를 각각 임시 폴더에 단독 복사하고 작업 디렉터리도 그 폴더로 지정했다. 앱 자식 프로세스의 PATH를 Windows와 System32로 제한해 실행했으며 화면 표시와 Rust `get_state` 응답이 통과했다. Node 모듈 경로·개발 도구 환경 변수는 전달하지 않았다.
- 재현 명령: `node scripts/check-runtime.mjs` 또는 `node scripts/check-runtime.mjs "C:\Program Files\NexaTerm\nexaterm.exe"`. 이 명령의 Node.js·Playwright는 검증기를 실행하기 위한 도구이며 검사 대상 앱의 실행 의존성이 아니다. debug 실행의 데이터와 WebView 캐시는 임시 디렉터리에 격리한다. release 실행은 기존 앱 데이터 경로를 사용하므로 저장 기능은 호출하지 않는다.
- 이 검사는 WebView2가 이미 설치된 개발 PC에서 수행했다. 개발 도구가 실제 미설치된 별도 PC/VM에서 설치·SSH·SFTP까지 확인한 결과는 아니다.
- 현재 `webviewInstallMode`는 `downloadBootstrapper`이다. WebView2가 없으면 설치 시 인터넷이 필요하고, 실행 파일 단독 배포는 WebView2를 자동 설치하지 않는다. 오프라인 배포가 필요하면 별도 런타임 또는 설치 모드를 준비해야 한다. [Tauri Windows 설치 문서](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options)를 참고한다.

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
