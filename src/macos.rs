use crate::SelectedText;
use accessibility_ng::{AXAttribute, AXUIElement};
use accessibility_sys_ng::{kAXFocusedUIElementAttribute, kAXSelectedTextAttribute};
use active_win_pos_rs::get_active_window;
use core_foundation::string::CFString;
use lru::LruCache;
use parking_lot::Mutex;
use std::num::NonZeroUsize;

static GET_SELECTED_TEXT_METHOD: Mutex<Option<LruCache<String, u8>>> = Mutex::new(None);

// TDO: optimize / refactor / test later
fn split_file_paths(input: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut current_path = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '\'' => {
                current_path.push(ch);
                in_quotes = !in_quotes;
                if !in_quotes {
                    paths.push(current_path.clone());
                    current_path.clear();
                }
            }
            ' ' if !in_quotes => {
                if !current_path.is_empty() {
                    paths.push(current_path.clone());
                    current_path.clear();
                }
            }
            _ => current_path.push(ch),
        }
    }

    if !current_path.is_empty() {
        paths.push(current_path);
    }

    paths
}

pub async fn get_selected_text<F>(after_paste_fn: F) -> Option<SelectedText>
where
    F: Fn(),
{
    if GET_SELECTED_TEXT_METHOD.lock().is_none() {
        let cache = LruCache::new(NonZeroUsize::new(100).unwrap());
        *GET_SELECTED_TEXT_METHOD.lock() = Some(cache);
    }
    // let mut cache = GET_SELECTED_TEXT_METHOD.lock();
    // let cache = cache.as_mut().unwrap();
    // let cache = GET_SELECTED_TEXT_METHOD.lock().as_ref().unwrap().clone();
    let app_name = match get_active_window() {
        Ok(window) => window.app_name,
        Err(_) => {
            // user might be in the desktop / home view
            String::new()
        }
    };

    if app_name == "Finder" || app_name.is_empty() {
        if let Some(text) = get_selected_file_paths_by_clipboard_using_applescript() {
            return Some(SelectedText {
                is_file_paths: true,
                app_name: app_name,
                text: split_file_paths(&text),
            });
        }
    }

    let mut selected_text = SelectedText {
        is_file_paths: false,
        app_name: app_name.clone(),
        text: vec![],
    };

    {
        let mut cache = GET_SELECTED_TEXT_METHOD.lock().clone().unwrap();
        if let Some(text) = cache.get(&app_name) {
            if *text == 0 {
                let ax_text = get_selected_text_by_ax().unwrap_or_default();
                if !ax_text.is_empty() {
                    cache.put(app_name.clone(), 0);
                    selected_text.text = vec![ax_text];
                    return Some(selected_text);
                }
            }
            let txt = get_selected_text_by_clipboard_using_applescript(after_paste_fn).await.unwrap_or_default();
            selected_text.text = vec![txt];
            return Some(selected_text);
        }
        match get_selected_text_by_ax() {
            Some(txt) => {
                if !txt.is_empty() {
                    cache.put(app_name.clone(), 0);
                }
                selected_text.text = vec![txt];
                Some(selected_text)
            }
            None => match get_selected_text_by_clipboard_using_applescript(after_paste_fn).await {
                Some(txt) => {
                    if !txt.is_empty() {
                        cache.put(app_name, 1);
                    }
                    selected_text.text = vec![txt];
                    Some(selected_text)
                }
                None => None
            },
        }
    }
}

fn get_selected_text_by_ax() -> Option<String> {
    // debug_println!("get_selected_text_by_ax");
    let system_element = AXUIElement::system_wide();
    let Some(selected_element) = system_element
        .attribute(&AXAttribute::new(&CFString::from_static_string(
            kAXFocusedUIElementAttribute,
        )))
        .map(|element| element.downcast_into::<AXUIElement>())
        .ok()
        .flatten()
    else {
        return None;
    };
    let Some(selected_text) = selected_element
        .attribute(&AXAttribute::new(&CFString::from_static_string(
            kAXSelectedTextAttribute,
        )))
        .map(|text| text.downcast_into::<CFString>())
        .ok()
        .flatten()
    else {
        return None
    };
    Some(selected_text.to_string())
}

const REGULAR_TEXT_COPY_APPLE_SCRIPT_SNIPPET_1: &str = r#"
use AppleScript version "2.4"
use scripting additions
use framework "Foundation"
use framework "AppKit"

set savedAlertVolume to alert volume of (get volume settings)

-- Back up clipboard contents:
-- set savedClipboard to the clipboard

-- set thePasteboard to current application's NSPasteboard's generalPasteboard()
-- set theCount to thePasteboard's changeCount()

tell application "System Events"
    set volume alert volume 0
end tell

-- Copy selected text to clipboard:
tell application "System Events" to keystroke "c" using {command down}
-- delay 0.1 -- Without this, the clipboard may have stale data.

tell application "System Events"
    set volume alert volume savedAlertVolume
end tell

-- if thePasteboard's changeCount() is theCount then
--     return ""
-- end if

-- set theSelectedText to the clipboard

-- set the clipboard to savedClipboard

-- theSelectedText
"#;

const REGULAR_TEXT_COPY_APPLE_SCRIPT_SNIPPET_2: &str = r#"
use AppleScript version "2.4"
use scripting additions
use framework "Foundation"
use framework "AppKit"

-- set savedAlertVolume to alert volume of (get volume settings)

-- Back up clipboard contents:
set savedClipboard to the clipboard

set thePasteboard to current application's NSPasteboard's generalPasteboard()
set theCount to thePasteboard's changeCount()

-- tell application "System Events"
--    set volume alert volume 0
-- end tell

-- Copy selected text to clipboard:
-- tell application "System Events" to keystroke "c" using {command down}
delay 0.1 -- Without this, the clipboard may have stale data.

-- tell application "System Events"
--     set volume alert volume savedAlertVolume
-- end tell

if thePasteboard's changeCount() is theCount then
    return ""
end if

set theSelectedText to the clipboard

set the clipboard to savedClipboard

theSelectedText
"#;

const FILE_PATH_COPY_APPLE_SCRIPT: &str = r#"
use AppleScript version "2.4"
use scripting additions
use framework "Foundation"
use framework "AppKit"

set savedAlertVolume to alert volume of (get volume settings)

-- Back up clipboard contents:
set savedClipboard to the clipboard

set thePasteboard to current application's NSPasteboard's generalPasteboard()
set theCount to thePasteboard's changeCount()

tell application "System Events"
    set volume alert volume 0
end tell

-- Copy selected text to clipboard:
tell application "System Events" to keystroke "c" using {command down, option down}
delay 0.1 -- Without this, the clipboard may have stale data.

tell application "System Events"
    set volume alert volume savedAlertVolume
end tell

if thePasteboard's changeCount() is theCount then
    return ""
end if

set theSelectedText to the clipboard

set the clipboard to savedClipboard

theSelectedText
"#;

async fn get_selected_text_by_clipboard_using_applescript<F>(after_paste_fn: F) -> Option<String>
where
    F: Fn(),
{
    // debug_println!("get_selected_text_by_clipboard_using_applescript");
    let (sender, receiver) = tokio::sync::oneshot::channel();

    // Fetch text from the clipboard, and there is a delay while waiting for the copy to be ready
    tokio::spawn(async move {
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(REGULAR_TEXT_COPY_APPLE_SCRIPT_SNIPPET_2)
            .output()
            .ok();
        let _ = sender.send(output);
    });

    after_paste_fn();

    // Set selected text to clipboard
    tokio::spawn(async move {
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(REGULAR_TEXT_COPY_APPLE_SCRIPT_SNIPPET_1)
            .output()
            .ok();
    });

    let output = receiver.await.unwrap_or(None);
    if let Some(output_value) = output {
        if output_value.status.success() {
            let content = String::from_utf8(output_value.stdout).ok().unwrap_or_default();
            let content = content.trim();
            Some(content.to_string())
        } else {
            // let err = output_value
            //     .stderr
            //     .into_iter()
            //     .map(|c| c as char)
            //     .collect::<String>()
            //     .into();
            None
        }
    } else {
        None
    }
}

fn get_selected_file_paths_by_clipboard_using_applescript() -> Option<String> {
    // debug_println!("get_selected_text_by_clipboard_using_applescript");
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(FILE_PATH_COPY_APPLE_SCRIPT)
        .output()
        .ok()?;
    if output.status.success() {
        let content = String::from_utf8(output.stdout).ok().unwrap_or_default();
        let content = content.trim();
        Some(content.to_string())
    } else {
        // let err = output
        //     .stderr
        //     .into_iter()
        //     .map(|c| c as char)
        //     .collect::<String>()
        //     .into();
        None
    }
}
