mod backup;
mod commands;
mod exit_confirmation;
mod file_commands;
mod models;
mod secrets;
mod sftp;
mod ssh;
#[cfg(test)]
mod ssh_tests;
#[cfg(test)]
mod storage_tests;
mod store;

use tauri::Manager;
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = if cfg!(debug_assertions) {
                std::env::var_os("NEXATERM_TEST_DATA_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or(app.path().app_data_dir()?)
            } else {
                app.path().app_data_dir()?
            };
            std::fs::create_dir_all(&directory)?;
            let store = store::Store::open(&directory.join("settings.db"))
                .map_err(std::io::Error::other)?;
            app.manage(commands::AppState::new(store));
            #[cfg(debug_assertions)]
            if let Some(root) = std::env::var_os("NEXATERM_TEST_LOCAL_ROOT") {
                app.state::<commands::AppState>()
                    .runtime
                    .files
                    .grant_root(std::path::Path::new(&root))
                    .map_err(std::io::Error::other)?;
            }
            tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .on_navigation(commands::allowed_url)
                .build()?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<commands::AppState>();
                let has_sessions = state
                    .runtime
                    .sessions
                    .lock()
                    .is_ok_and(|sessions| !sessions.is_empty());
                let confirm_exit = state.store.read().map(|doc| doc.settings.confirm_exit).unwrap_or(true);
                if has_sessions && confirm_exit {
                    api.prevent_close();
                    if !state
                        .closing
                        .swap(true, std::sync::atomic::Ordering::SeqCst)
                    {
                        let window = window.clone();
                        tauri::async_runtime::spawn(async move {
                            let decision = exit_confirmation::ask(&window).await;
                            let state = window.state::<commands::AppState>();
                            let mut should_exit = false;
                            match decision {
                                Ok(decision) if decision.exit => {
                                    let saved = if decision.do_not_ask_again {
                                        state.store.set_confirm_exit(false).map(|_| ())
                                    } else {
                                        Ok(())
                                    };
                                    match saved {
                                        Ok(()) => should_exit = true,
                                        Err(error) => {
                                            rfd::AsyncMessageDialog::new()
                                                .set_title("종료 설정 저장 실패")
                                                .set_description(format!("{error}\n설정을 저장하지 못해 종료를 취소했습니다."))
                                                .show().await;
                                        }
                                    }
                                }
                                Ok(_) => {}
                                Err(error) => {
                                    let answer = rfd::AsyncMessageDialog::new()
                                        .set_title("NexaTerm 종료")
                                        .set_description(format!("{error}\n활성 연결과 전송을 모두 종료하고 앱을 닫을까요?"))
                                        .set_buttons(rfd::MessageButtons::YesNo)
                                        .show().await;
                                    should_exit = answer == rfd::MessageDialogResult::Yes;
                                }
                            }
                            if should_exit {
                                state.close_all();
                                let _ = window.destroy();
                            } else {
                                state
                                    .closing
                                    .store(false, std::sync::atomic::Ordering::SeqCst);
                            }
                        });
                    }
                }
            }
            if let tauri::WindowEvent::Destroyed = event
                && let Some(state) = window.try_state::<commands::AppState>()
            {
                state.close_all();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_profile,
            commands::delete_profile,
            commands::save_settings,
            commands::export_backup,
            commands::preview_backup,
            commands::restore_backup,
            commands::cancel_restore,
            commands::connect_session,
            commands::terminal_input,
            commands::terminal_resize,
            commands::disconnect_session,
            commands::resolve_host_key,
            commands::terminal_ack,
            commands::choose_private_key,
            file_commands::local_roots,
            file_commands::open_local_location,
            file_commands::choose_local_root,
            file_commands::local_list,
            file_commands::remote_list,
            file_commands::transfer_queue,
            file_commands::enqueue_transfer,
            file_commands::cancel_transfer,
            file_commands::retry_transfer,
            file_commands::resolve_transfer_conflict,
            file_commands::file_operation,
            file_commands::reset_sftp
        ])
        .run(tauri::generate_context!())
        .expect("NexaTerm application initialization failed");
}

#[cfg(test)]
mod security_tests {
    #[test]
    fn navigation_and_ipc_origin_policy() {
        use crate::commands::allowed_url;
        assert!(allowed_url(
            &tauri::Url::parse("http://tauri.localhost/#home").unwrap()
        ));
        for url in [
            "https://evil.example/",
            "http://tauri.localhost.evil/",
            "http://tauri.localhost/other",
            "http://tauri.localhost/?inject=1",
            "http://tauri.localhost:9876/",
            "http://user@tauri.localhost/",
            "file:///C:/app.html",
        ] {
            assert!(!allowed_url(&tauri::Url::parse(url).unwrap()));
        }
    }
}
