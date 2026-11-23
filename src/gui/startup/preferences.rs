use crate::admin_startup::Mode;
use crate::config::Config;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Preferences {
    pub normal: bool,
    pub elevated: bool,
    pub tray: bool,
    pub authorized: String,
}

impl Preferences {
    pub fn capture(config: &Config) -> Self {
        Self {
            normal: config.run_at_startup,
            elevated: config.run_as_admin_on_startup,
            tray: config.start_in_tray,
            authorized: config.authorized_startup_path.clone(),
        }
    }

    pub fn write(&self, config: &mut Config) {
        config.run_at_startup = self.normal;
        config.run_as_admin_on_startup = self.elevated;
        config.start_in_tray = self.tray;
        config.authorized_startup_path.clone_from(&self.authorized);
    }

    pub fn adopt(&mut self, path: &Path) {
        if let Some(path) = path.to_str() {
            self.authorized = path.to_owned();
        }
    }

    pub fn set_mode(&mut self, mode: Mode, path: &Path) {
        self.normal = matches!(mode, Mode::Normal);
        self.elevated = matches!(mode, Mode::Elevated);
        if matches!(mode, Mode::Off) {
            self.tray = false;
        }
        self.adopt(path);
    }
}
