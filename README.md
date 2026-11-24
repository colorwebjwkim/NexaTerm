# NexaTerm

Windows용 SSH 터미널 및 SFTP 클라이언트입니다. 여러 서버에 독립된 탭으로 접속하고, 터미널 작업과 파일 전송을 한 앱에서 처리할 수 있습니다.

Rust와 Tauri 2, Svelte, TypeScript, xterm.js로 구성되어 있습니다. 현재 소스 버전은 **0.3.3**이며 개발·검토 단계입니다.

## 주요 기능

- **다중 SSH 탭**: 같은 서버에도 독립된 연결을 열고, 탭 드래그로 순서를 바꿀 수 있습니다.
- **서버 프로필 관리**: 프로필 생성·편집·검색, 그룹·태그·즐겨찾기를 지원합니다.
- **SFTP 파일 관리자**: 로컬·원격 듀얼 패널에서 파일과 폴더를 업로드·다운로드합니다. 터미널 없이 SFTP만 여는 접속 방식도 제공합니다.
- **전송 관리**: 진행률, 취소·재시도, 파일 충돌 처리와 탭을 닫은 뒤 전송 유지를 지원합니다.
- **접속 정보 보호**: 저장을 선택한 비밀번호는 Windows DPAPI로 암호화하며, SSH 서버 호스트키는 최초 확인과 변경 시 재승인을 요구합니다.
- **암호화 백업·복원**: 프로필과 설정을 `.nxtbackup` 파일로 저장하고, 선택적으로 비밀번호를 포함할 수 있습니다.
- **화면 설정**: Windows 테마를 따르는 자동 모드와 다크·라이트 수동 선택, 터미널 글꼴 크기, 스크롤 버퍼와 서버 목록 접기·펼치기를 제공합니다.

## 실행 환경

- Windows 11 x64
- Microsoft Edge WebView2 Runtime

빌드된 앱을 실행할 때 Node.js·npm·pnpm, Rust·Cargo, Visual Studio Build Tools, Windows SDK, Git은 필요하지 않습니다. 아래 개발 도구 요구 사항은 소스에서 빌드할 때만 적용됩니다. 현재 배포물은 코드 서명이 없는 개발·검토용입니다.

WebView2가 없는 PC에서는 현재 설치 프로그램이 인터넷으로 런타임을 내려받아 설치합니다. 실행 파일만 복사해 사용하는 경우 WebView2가 미리 설치되어 있어야 합니다. WebView2가 없는 오프라인 PC에는 별도의 오프라인 런타임 설치가 필요합니다.

[GitHub Releases](https://github.com/colorwebjwkim/NexaTerm/releases/latest)에서 Windows x64용 NSIS 설치 파일(`*-setup.exe`) 또는 MSI를 내려받아 실행합니다. `SHA256SUMS.txt`로 파일 무결성을 확인할 수 있습니다. 소스에서 직접 빌드하려면 아래 개발 환경과 명령을 사용합니다.

## 사용 방법

1. `서버 프로필 만들기`에서 호스트, 포트, 사용자명과 인증 정보를 입력합니다.
2. `접속 방식`으로 SSH 또는 SFTP 전용을 선택하고 저장합니다.
3. 서버 목록에서 프로필을 선택한 뒤 서버 호스트키 지문을 확인하고 연결합니다.
4. SSH 연결 중 파일을 전송하려면 `파일 열기 (SFTP)` 또는 `F9`를 누릅니다.
5. 로컬·원격 파일을 선택해 업로드하거나 다운로드합니다. 같은 이름의 파일은 충돌 처리 방식을 선택합니다.

SFTP 목록은 행의 이름·크기·수정일 어디를 클릭해도 선택됩니다. `Ctrl+클릭`으로 개별 선택·해제, `Shift+클릭`으로 연속 범위 선택, `Ctrl+Shift+클릭`으로 범위 추가가 가능합니다. 마우스로 영역을 드래그하면 여러 항목을 선택하며, `Ctrl+드래그`는 영역 안 항목의 선택을 반전합니다. 빈 공간을 클릭하면 선택을 해제하고, 폴더는 더블클릭으로 엽니다. 체크박스로도 선택·해제할 수 있습니다.

SFTP 목록에 초점이 있을 때 `↑`·`↓`, `Home`·`End`, `Page Up`·`Page Down`으로 이동·선택합니다. `Shift`를 함께 누르면 범위를 선택하고, `Ctrl`을 함께 누르면 기존 선택을 유지한 채 초점만 이동합니다. `Ctrl+Space`로 초점 항목을 선택·해제하고, `Ctrl+Shift`와 이동 키로 범위를 추가합니다. `Ctrl+A`는 표시된 선택 가능 항목 전체 선택, `Esc`는 선택 해제입니다. 이름을 입력하면 일치하는 항목을 찾아 선택합니다. `Enter`로 폴더를 열고 `Alt+↑` 또는 `Backspace`로 상위 폴더로 이동합니다.

| 단축키 | 동작 |
| --- | --- |
| `Ctrl+T` | 서버 프로필 생성 |
| `Ctrl+W` | 현재 탭 닫기 |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | 다음 / 이전 탭 |
| `F9` | SSH 연결의 SFTP 패널 열기 |
| `Ctrl+마우스 휠` | 터미널 글꼴 크기 조절 |
| `F1` | 단축키 도움말 |
| `Alt+F4` | 앱 종료 (터미널에 초점이 있어도 동작) |

연결이 실패하거나 종료된 탭에서는 `Enter`로 재접속할 수 있습니다. 상세 사용 흐름과 데이터 저장 위치는 [개발 및 실행 안내](docs/DEVELOPMENT.md)를 참고하세요.

일반 화면의 메뉴·서버 목록·안내 문구는 드래그해도 텍스트가 선택되지 않습니다. 터미널 출력, 입력창, 오류 메시지와 호스트키 지문은 선택·복사할 수 있습니다.

오른쪽 클릭 메뉴는 터미널 안에서만 허용합니다. 입력창을 포함한 나머지 화면에서는 표시하지 않으며, 복사·붙여넣기 단축키는 사용할 수 있습니다.

`보기 → Windows 테마 따르기`로 자동 모드를 바로 켤 수 있습니다. `도구 → 환경 설정 → 테마`에서는 자동 모드와 다크·라이트 수동 모드를 선택합니다. 자동 모드는 Windows 앱 색상을 화면·터미널·제목 표시줄에 반영하며 실행 중에도 갱신됩니다. 새 설정의 기본값은 자동 모드입니다.

## 소스에서 실행하기

프로젝트에 고정된 도구 버전을 사용합니다.

| 도구 | 요구 사항 |
| --- | --- |
| Node.js | 24 이상 |
| pnpm | 11.25.0 |
| Rust | 1.99.0, MSVC x64 도구 체인 |
| Visual Studio Build Tools | C++ 데스크톱 개발 도구와 Windows SDK |
| WebView2 | Runtime 설치 |

프로젝트 루트에서 PowerShell로 실행합니다.

```powershell
pnpm install --frozen-lockfile
pnpm dev
```

## 빌드 및 검증

```powershell
# 정적 검사와 Rust 테스트
pnpm lint
pnpm typecheck
pnpm test
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

# Windows 실행 파일 빌드
pnpm build

# 빌드한 release 앱 실행 검사
pnpm test:release

# MSI 설치 파일 생성: 소스 재컴파일 포함
pnpm exec tauri build --bundles msi --ci

# NSIS 설치 파일 생성
pnpm dist:win
```

생성 경로는 다음과 같습니다.

- 실행 파일: `src-tauri/target/release/nexaterm.exe`
- MSI: `src-tauri/target/release/bundle/msi/`
- NSIS: `src-tauri/target/release/bundle/nsis/`

Windows WebView E2E 실행 방법은 [개발 문서](docs/DEVELOPMENT.md)를 참고하세요. 빌드 성공과 실제 설치·실행·원격 서버 호환 검증은 별도로 확인해야 합니다.

## 지원 범위

- SSH 비밀번호 인증과 지원되는 개인키 인증을 제공합니다. RSA 서버 호스트키를 지원하지만 **RSA 개인키 인증은 지원하지 않습니다**.
- 기본 호스트키 설정은 구형 서버 호환을 위한 `ssh-rsa 우선`입니다. 프로필에서 RSA-SHA2, Ed25519, ECDSA 등 개별 알고리즘을 선택할 수 있습니다.
- Telnet, SSH Agent, keyboard-interactive/MFA, 점프호스트는 아직 지원하지 않습니다.
- SFTP는 파일·폴더 재귀 전송을 지원하지만, 비어 있지 않은 폴더 삭제와 드래그 앤 드롭 전송은 지원하지 않습니다.
- macOS와 Linux는 현재 지원 대상이 아닙니다.

저장된 비밀번호는 Windows 사용자·PC에 결합되므로 다른 PC로 옮길 때는 앱의 암호화 백업·복원을 사용하세요. 백업 암호를 분실하면 복구할 수 없습니다. 인증·암호화의 적용 범위와 알려진 제약은 [보안 문서](docs/SECURITY.md)에 정리되어 있습니다.

## 문서

- [개발 환경과 상세 사용법](docs/DEVELOPMENT.md)
- [아키텍처](docs/ARCHITECTURE.md)
- [보안 구현과 한계](docs/SECURITY.md)
- [구현·검증 이력과 남은 작업](docs/ROADMAP.md)
- [0.3.3 검증 기록](docs/VALIDATION-0.3.3.md)
- [아이콘과 UI 색상](docs/BRANDING.md)
- [프로젝트 관리 — 요구사항·개발 이력·검증 범위](PROJECT.md)

프로젝트 작업 시에는 루트의 `AGENTS.md` 지침에 따라 이 README와 `PROJECT.md`를 함께 확인합니다. `PROJECT.md`의 과거 계획과 날짜별 기록은 현재 지원 기능과 구분합니다.
