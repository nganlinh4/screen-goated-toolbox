use super::CookieBrowser;
use anyhow::{Result, anyhow};
use std::sync::mpsc::{self, Receiver, TryRecvError};

enum State {
    NotStarted,
    Running(Receiver<Result<Vec<CookieBrowser>>>),
    Complete,
}

pub(super) struct BrowserDiscovery {
    state: State,
    available: Vec<CookieBrowser>,
}

impl BrowserDiscovery {
    pub fn new() -> Self {
        Self {
            state: State::NotStarted,
            available: vec![CookieBrowser::None],
        }
    }

    pub fn available(&self) -> &[CookieBrowser] {
        &self.available
    }

    pub fn update(&mut self, ctx: &eframe::egui::Context) {
        let ctx = ctx.clone();
        if let Err(error) =
            self.update_with(super::detection::detect_installed_browsers, move || {
                ctx.request_repaint();
            })
        {
            crate::log_info!("[Downloader] Browser discovery failed: {error:#}");
        }
    }

    fn update_with(
        &mut self,
        detect: impl FnOnce() -> Vec<CookieBrowser> + Send + 'static,
        wake: impl FnOnce() + Send + 'static,
    ) -> Result<()> {
        match std::mem::replace(&mut self.state, State::Complete) {
            State::NotStarted => {
                let (sender, receiver) = mpsc::sync_channel(1);
                std::thread::Builder::new()
                    .name("downloader-browser-discovery".into())
                    .spawn(move || {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(detect))
                            .map_err(|_| anyhow!("browser discovery worker panicked"));
                        let _ = sender.send(result);
                        wake();
                    })?;
                self.state = State::Running(receiver);
            }
            State::Running(receiver) => match receiver.try_recv() {
                Ok(result) => self.available = result?,
                Err(TryRecvError::Empty) => self.state = State::Running(receiver),
                Err(TryRecvError::Disconnected) => {
                    return Err(anyhow!("browser discovery worker disconnected"));
                }
            },
            State::Complete => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
