//! Serial ownership for the continuous-image worker.
use crate::overlay::image_capture_target::ImageCaptureTarget;

#[derive(Clone)]
pub(super) struct Request {
    pub target: ImageCaptureTarget,
    pub hotkey_name: String,
    pub hotkey_id: i32,
    pub trigger_vk: u32,
}

struct Owner {
    id: u64,
    request: Request,
    stopping: bool,
    ready: bool,
    thread_id: u32,
}

#[derive(Default)]
pub(super) struct Lifecycle {
    next_id: u64,
    owner: Option<Owner>,
    pending: Option<Request>,
}

pub(super) enum Admission {
    Start(u64, Request),
    Queued,
    Ignored,
    Exhausted,
}

impl Lifecycle {
    fn reserve(&mut self, request: Request) -> Admission {
        let Some(id) = self.next_id.checked_add(1) else {
            return Admission::Exhausted;
        };
        self.next_id = id;
        self.owner = Some(Owner {
            id,
            request: request.clone(),
            stopping: false,
            ready: false,
            thread_id: 0,
        });
        Admission::Start(id, request)
    }

    pub fn enter(&mut self, request: Request) -> Admission {
        match self.owner.as_ref() {
            None => self.reserve(request),
            Some(owner) if owner.stopping => {
                self.pending = Some(request);
                Admission::Queued
            }
            Some(_) => Admission::Ignored,
        }
    }

    pub fn live(&self, id: u64) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| owner.id == id && !owner.stopping)
    }

    pub fn owns(&self, id: u64) -> bool {
        self.owner.as_ref().is_some_and(|owner| owner.id == id)
    }

    pub fn running(&self) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|owner| owner.ready && !owner.stopping)
    }

    pub fn desired_binding(&self) -> Option<(ImageCaptureTarget, i32, bool)> {
        if let Some(request) = &self.pending {
            return Some((request.target, request.hotkey_id, false));
        }
        self.owner
            .as_ref()
            .filter(|owner| !owner.stopping)
            .map(|owner| (owner.request.target, owner.request.hotkey_id, owner.ready))
    }

    pub fn begin_gesture(&self, gesture: &super::gesture::Gesture) -> Option<bool> {
        self.running().then(|| gesture.start())
    }

    pub fn register_thread(&mut self, id: u64, thread_id: u32) -> bool {
        let Some(owner) = self.owner.as_mut() else {
            return false;
        };
        if owner.id != id || owner.stopping {
            return false;
        }
        owner.thread_id = thread_id;
        true
    }

    pub fn ready(&mut self, id: u64) -> bool {
        let Some(owner) = self.owner.as_mut() else {
            return false;
        };
        if owner.id != id || owner.stopping {
            return false;
        }
        owner.ready = true;
        true
    }

    pub fn stop(&mut self) -> Option<u32> {
        self.pending = None;
        let owner = self.owner.as_mut()?;
        if owner.stopping {
            return None;
        }
        owner.stopping = true;
        Some(owner.thread_id)
    }

    pub fn finish(&mut self, id: u64) -> Admission {
        if !self.owner.as_ref().is_some_and(|owner| owner.id == id) {
            return Admission::Ignored;
        }
        self.owner = None;
        match self.pending.take() {
            Some(request) => self.reserve(request),
            None => Admission::Ignored,
        }
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
