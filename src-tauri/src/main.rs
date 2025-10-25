// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod log;
mod utils;
mod events;
mod overlay;
mod monitors;
mod brightness;

fn main() {
    crate::app::run();
}
