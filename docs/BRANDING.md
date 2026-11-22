# NexaTerm 아이콘과 UI 색상

앱 아이콘은 터미널 창과 `>_` 프롬프트 모양인 `src/renderer/assets/nexaterm-icon.svg`를 사용한다. favicon은 이 SVG를 사용하고, 실행 파일·설치 바로가기용 네이티브 아이콘은 `pnpm exec tauri icon src/renderer/assets/nexaterm-icon.svg --output src-tauri/icons`로 생성한다. 상단은 파일·연결·보기·도구·도움말 메뉴로 구성한다.

UI 색상은 `src/renderer/styles/app.css`에서 관리한다. 다크 UI는 청록색, 라이트 UI는 파란색을 동작색으로 사용하며 성공·주의·오류는 용도별 색상으로 구분한다. 터미널 배경·커서·선택 영역은 `src/renderer/components/TerminalView.svelte`에서 관리한다. 서버가 출력하는 ANSI 색상은 유지한다.

`src-tauri/build.rs`는 `icons/icon.ico` 변경 시 Windows 실행 파일의 아이콘 리소스를 다시 컴파일한다. 아이콘을 교체할 때는 MSI 패키징뿐 아니라 실행 파일까지 재빌드하고, 실행 파일에 포함된 아이콘도 확인한다.
