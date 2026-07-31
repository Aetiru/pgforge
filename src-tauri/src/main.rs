// En release no se abre una consola detrás de la ventana en Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    pgforge_app_lib::run()
}
