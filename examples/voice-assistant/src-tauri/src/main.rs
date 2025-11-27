//! Voice Assistant Tauri application entry point

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    voice_assistant_lib::run();
}
