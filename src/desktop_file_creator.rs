/// This is used to get around limitations that wayland creates in terms of displaying
/// an application's icon.
///
/// It is pretty simple, if the file doesn't exist, then it will create a temporary one
/// in the user's local xdg directory. If it *does* exist, then it will not delete
/// the file
use std::{fs::File, io::Write};
const ICON_PATH: &str = "icons/kanbanrs_icon.png";
const DESKTOP_FILE_PATH: &str = "applications/kanbanrs.desktop";
pub fn needs_desktop_file() -> bool {
    let exec_path: String = std::env::current_exe()
        .expect("Could not get current executable path")
        .to_str()
        .expect("Could not unwrap executable path?")
        .to_owned();
    if std::fs::exists("/usr/share/applications/kanbanrs.desktop")
        .expect("Couldn't check for desktop file existence")
    {
        !std::fs::read("/usr/share/applications/kanbanrs.desktop").is_ok_and(|x| {
            let x: &str = str::from_utf8(&x).expect("Could not decode desktop file");
            !x.contains(&exec_path)
        })
    } else {
        let xdg = xdg::BaseDirectories::new().expect("Could not get xdg directories");
        let local_desktop_file = xdg.find_data_file(DESKTOP_FILE_PATH);
        if let Some(ldf) = local_desktop_file {
            std::fs::read(ldf).is_ok_and(|x| {
                let x: &str = str::from_utf8(&x).expect("Could not decode desktop file");
                !x.contains(&exec_path)
            })
        } else {
            false
        }
    }
}
fn cleanup_temporary_desktop_files() {
    let xdg = xdg::BaseDirectories::new().expect("Could not get xdg directories");
    std::fs::remove_file(xdg.find_data_file(DESKTOP_FILE_PATH).unwrap()).unwrap();
    std::fs::remove_file(xdg.find_data_file(ICON_PATH).unwrap()).unwrap();
}
pub struct TemporaryDesktopCleanupHandle {
    needs_cleanup: bool,
}
impl Drop for TemporaryDesktopCleanupHandle {
    fn drop(&mut self) {
        if self.needs_cleanup {
            cleanup_temporary_desktop_files();
        }
    }
}
pub fn create_dot_desktop_file() -> TemporaryDesktopCleanupHandle {
    let xdg = xdg::BaseDirectories::new().unwrap();
    let ret_val = TemporaryDesktopCleanupHandle {
        needs_cleanup: !needs_desktop_file(),
    };
    if ret_val.needs_cleanup {
        let icon_path = xdg.place_data_file(ICON_PATH).unwrap();
        let mut icon_file = File::create(&icon_path).unwrap();
        icon_file
            .write_all(crate::ICON_DATA)
            .expect("Could not write icon file");
        let desktop_file_path = xdg.place_data_file(DESKTOP_FILE_PATH).unwrap();
        let mut desktop_file = File::create(&desktop_file_path).unwrap();
        desktop_file
            .write_fmt(format_args!(
                include_str!("../assets/desktop_template.desktop"),
                std::env::current_exe().unwrap().display(),
                std::env::current_exe().unwrap().display(),
                icon_path.display(),
            ))
            .expect("Could not write desktop template");
    }
    ret_val
}
