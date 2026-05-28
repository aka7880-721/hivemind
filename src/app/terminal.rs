// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License

use std::{
    io,
    process::{Command, Stdio},
};

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use anyhow::{Context, Result};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    ENABLE_ECHO_INPUT, ENABLE_EXTENDED_FLAGS, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, GetConsoleMode, SetConsoleMode,
};

use super::{App, StatusMessage};

pub fn run() -> Result<()> {
    restore_console_input_mode();
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal);
    let restore_result = restore_terminal(&mut terminal);
    restore_console_input_mode();
    restore_result?;
    result
}

pub(crate) fn restore_console_input_mode() {
    #[cfg(windows)]
    {
        let handle = io::stdin().as_raw_handle();
        if handle.is_null() {
            return;
        }

        let mut mode = 0;
        let stdin = handle as windows_sys::Win32::Foundation::HANDLE;

        unsafe {
            if GetConsoleMode(stdin, &mut mode) == 0 {
                return;
            }

            let restored_mode = (mode
                | ENABLE_PROCESSED_INPUT
                | ENABLE_LINE_INPUT
                | ENABLE_ECHO_INPUT
                | ENABLE_EXTENDED_FLAGS)
                & !ENABLE_VIRTUAL_TERMINAL_INPUT;

            let _ = SetConsoleMode(stdin, restored_mode);
        }
    }
}

pub(crate) fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )
    .context("failed to enter the alternate screen")?;
    Terminal::new(CrosstermBackend::new(stdout)).context("failed to create terminal")
}

pub(crate) fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )
    .context("failed to leave the alternate screen")?;
    terminal
        .show_cursor()
        .context("failed to show the cursor")?;
    Ok(())
}

pub(crate) fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new()?;
    app.prime_selected_project_dashboard_data();
    let mut needs_draw = true;

    while !app.should_quit {
        if needs_draw {
            terminal.draw(|frame| app.draw(frame))?;
            needs_draw = false;
        }

        match app.try_finish_background_job() {
            Ok(true) => {
                needs_draw = true;
                continue;
            }
            Ok(false) => {}
            Err(error) => {
                app.status = StatusMessage::error(error.to_string());
                needs_draw = true;
                continue;
            }
        }

        if event::poll(app.next_poll_timeout()).context("event polling failed")? {
            match event::read().context("event read failed")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if let Err(error) = app.handle_key(key) {
                        app.status = StatusMessage::error(error.to_string());
                    }
                    needs_draw = true;
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                    needs_draw = true;
                }
                Event::Paste(text) => {
                    app.handle_paste(text);
                    needs_draw = true;
                }
                Event::Resize(_, _) => needs_draw = true,
                Event::FocusGained | Event::FocusLost => {}
                Event::Key(_) => {}
            }
        } else if app.tick_ui_state() {
            needs_draw = true;
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub(crate) fn copy_text_via_linux_clipboard_cli(text: &str) -> bool {
    try_clipboard_stdin_command("wl-copy", &[], text)
        || try_clipboard_stdin_command("xclip", &["-selection", "clipboard"], text)
        || try_clipboard_stdin_command("xsel", &["--clipboard", "--input"], text)
}

#[cfg(target_os = "linux")]
pub(crate) fn paste_from_linux_clipboard_cli() -> Option<String> {
    try_clipboard_stdout_command("wl-paste", &[])
        .or_else(|| try_clipboard_stdout_command("xclip", &["-selection", "clipboard", "-o"]))
        .or_else(|| try_clipboard_stdout_command("xsel", &["--clipboard", "--output"]))
}

#[cfg(target_os = "linux")]
fn try_clipboard_stdout_command(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn try_clipboard_stdin_command(program: &str, args: &[&str], text: &str) -> bool {
    use std::io::Write;
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => return false,
    };
    if stdin.write_all(text.as_bytes()).is_err() {
        return false;
    }
    drop(stdin);
    matches!(child.wait(), Ok(status) if status.success())
}
