use crate::cody::init_cody_window;
use crate::common::{show_logs, show_window};
use tauri::api::shell;
use tauri::{
    AppHandle, CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu,
    SystemTrayMenuItem,
};

pub fn create_system_tray() -> SystemTray {
    SystemTray::new().with_menu(create_system_tray_menu())
}

fn create_system_tray_menu() -> SystemTrayMenu {
    SystemTrayMenu::new()
        .add_item(CustomMenuItem::new("open".to_string(), "Open Sourcegraph"))
        .add_item(CustomMenuItem::new("cody".to_string(), "Show Cody"))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(
            CustomMenuItem::new("settings".to_string(), "Settings").accelerator("CmdOrCtrl+,"),
        )
        .add_item(CustomMenuItem::new(
            "troubleshoot".to_string(),
            "Troubleshoot",
        ))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new(
            "about".to_string(),
            "About Sourcegraph",
        ))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("restart".to_string(), "Restart"))
        .add_item(CustomMenuItem::new("quit".to_string(), "Quit").accelerator("CmdOrCtrl+Q"))
}

pub fn on_system_tray_event(app: &AppHandle, event: SystemTrayEvent) {
    if let SystemTrayEvent::MenuItemClick { id, .. } = event {
        match id.as_str() {
            "open" => show_window(app, "main"),
            "cody" => {
                let win = app.get_window("cody");
                if win.is_none() {
                    init_cody_window(app);
                } else {
                    show_window(app, "cody")
                }
            }
            "settings" => {
                let window = app.get_window("main").unwrap();
                window.eval("window.location.href = '/settings'").unwrap();
                show_window(app, "main");
            }
            "troubleshoot" => show_logs(app),

            "about" => {
                shell::open(&app.shell_scope(), "https://about.sourcegraph.com", None).unwrap()
            }
            "restart" => app.restart(),
            "quit" => app.exit(0),
            _ => {}
        }
    }
}
