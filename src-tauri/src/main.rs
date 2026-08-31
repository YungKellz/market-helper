// Прячем консольное окно в релизной сборке под Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    caption_craft_lib::run()
}
