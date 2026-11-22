pub struct ExitDecision {
    pub exit: bool,
    pub do_not_ask_again: bool,
}

#[cfg(windows)]
pub async fn ask(window: &tauri::Window) -> Result<ExitDecision, String> {
    let parent = window.hwnd().map_err(|_| "종료 대상 창 확인 실패")?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || native_prompt(parent))
        .await
        .map_err(|_| "종료 확인 실행 실패")?
}

#[cfg(windows)]
fn native_prompt(parent: isize) -> Result<ExitDecision, String> {
    use windows_sys::Win32::{
        Foundation::HWND,
        UI::{
            Controls::{
                TASKDIALOGCONFIG, TDCBF_NO_BUTTON, TDCBF_YES_BUTTON, TDF_ALLOW_DIALOG_CANCELLATION,
                TDF_POSITION_RELATIVE_TO_WINDOW, TaskDialogIndirect,
            },
            WindowsAndMessaging::{IDNO, IDYES},
        },
    };
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    let title = wide("NexaTerm 종료");
    let instruction = wide("NexaTerm을 종료할까요?");
    let content = wide("활성 SSH/SFTP 연결과 진행 중인 전송이 모두 종료됩니다.");
    let verification = wide("다시 표시하지 않기 (환경 설정에서 다시 켤 수 있습니다)");
    let config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: parent as HWND,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION | TDF_POSITION_RELATIVE_TO_WINDOW,
        dwCommonButtons: TDCBF_YES_BUTTON | TDCBF_NO_BUTTON,
        pszWindowTitle: title.as_ptr(),
        pszMainInstruction: instruction.as_ptr(),
        pszContent: content.as_ptr(),
        pszVerificationText: verification.as_ptr(),
        nDefaultButton: IDNO,
        ..Default::default()
    };
    let mut button = IDNO;
    let mut checked = 0;
    // All UTF-16 strings and output pointers remain valid for the blocking dialog call.
    let result =
        unsafe { TaskDialogIndirect(&config, &mut button, std::ptr::null_mut(), &mut checked) };
    if result < 0 {
        return Err(format!("종료 확인 창 생성 실패: {result:#x}"));
    }
    Ok(ExitDecision {
        exit: button == IDYES,
        do_not_ask_again: button == IDYES && checked != 0,
    })
}

#[cfg(not(windows))]
pub async fn ask(_window: &tauri::Window) -> Result<ExitDecision, String> {
    let result = rfd::AsyncMessageDialog::new()
        .set_title("NexaTerm 종료")
        .set_description("활성 SSH/SFTP 연결과 진행 중인 전송을 모두 종료하고 앱을 닫을까요?")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        .await;
    Ok(ExitDecision {
        exit: result == rfd::MessageDialogResult::Yes,
        do_not_ask_again: false,
    })
}
