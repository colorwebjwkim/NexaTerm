# NexaTerm v0.3.3

Windows 11 x64용 SSH 터미널·SFTP 클라이언트의 첫 GitHub 릴리즈입니다. 2026-11-25 기준 최신 소스를 패키징했습니다.

## 주요 기능

- 독립 SSH 다중 탭, 탭 순서 드래그, 종료·실패한 탭에서 Enter 재접속.
- SFTP 전용 접속 및 로컬·원격 듀얼 패널 파일·폴더 전송.
- 탐색기 방식 SFTP 행·Ctrl/Shift·영역 드래그 선택과 키보드 탐색·전체 선택.
- Windows 테마 자동 반영, 터미널 글꼴 크기 조절과 Alt+F4 종료.
- DPAPI 비밀번호 보호, 서버 호스트키 확인과 암호화 설정 백업·복원.

## 다운로드 및 실행

- `NexaTerm_0.3.3_x64-setup.exe`: NSIS 설치 파일.
- `NexaTerm_0.3.3_x64_en-US.msi`: MSI 설치 파일.
- `SHA256SUMS.txt`: 설치 파일의 SHA-256 체크섬.

Microsoft Edge WebView2 Runtime이 필요합니다. 설치 파일은 필요 시 인터넷에서 WebView2를 내려받습니다. 앱 실행에는 Node.js·Rust 등 개발 도구가 필요하지 않습니다. 배포물에는 코드 서명이 없습니다.

다운로드한 파일의 해시는 PowerShell의 `Get-FileHash <파일 경로> -Algorithm SHA256` 결과를 `SHA256SUMS.txt`와 비교해 확인합니다.

## 검증 범위와 제한

- 이번 릴리즈 검사: 린트, 타입/Svelte 검사, Rust 테스트 17개, rustfmt, clippy, SFTP 선택 UI 회귀 2개.
- Windows x64 NSIS·MSI 빌드와 release WebView 시작·메뉴·프로필 창·Rust IPC smoke 검사를 통과했습니다.
- 이번 릴리즈 작업에서는 설치·업데이트·제거, 깨끗한 PC, 실제 원격 서버 연결과 전체 Windows WebView E2E를 다시 검증하지 않았습니다. 이전 E2E의 앱 재실행 후 무연결 Alt+F4 검사에는 미해결 실패가 있습니다.
- RSA 서버 호스트키는 지원하며 RSA 개인키 인증, Telnet, SSH Agent, MFA, 점프호스트는 지원하지 않습니다.
- SFTP 드래그 앤 드롭 전송과 비어 있지 않은 폴더 삭제는 지원하지 않습니다.

사용법과 상세 지원 범위는 [README](../README.md), 날짜별 검증 결과는 [PROJECT](../PROJECT.md)를 참고하세요.
