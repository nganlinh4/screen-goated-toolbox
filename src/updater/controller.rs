use super::lifecycle::{UpdateEvent, UpdateOwner};
use super::{UpdateStatus, download_and_stage, latest_candidate};
use crate::task_runtime::{TaskClass, TaskTicket};
use std::sync::{Arc, Mutex, mpsc::Sender};

#[derive(Default)]
struct Tasks {
    check: Option<TaskTicket>,
    download: Option<TaskTicket>,
}

pub struct Updater {
    owner: Arc<UpdateOwner>,
    tasks: Mutex<Tasks>,
}

impl Updater {
    pub fn new(tx: Sender<UpdateEvent>) -> Self {
        Self {
            owner: Arc::new(UpdateOwner::new(tx)),
            tasks: Mutex::new(Tasks::default()),
        }
    }

    pub fn accept_event(&self, event: UpdateEvent) -> Option<UpdateStatus> {
        self.owner.accept(event)
    }

    pub fn auto_check_for_updates(&self) {
        self.check(true);
    }

    pub fn check_for_updates(&self) {
        self.check(false);
    }

    fn check(&self, automatic: bool) {
        let Ok(mut tasks) = self.tasks.lock() else {
            return;
        };
        let Some(operation) = self.owner.begin_check(automatic) else {
            return;
        };
        let owner = Arc::clone(&self.owner);
        let task_operation = operation.clone();
        match crate::task_runtime::spawn(TaskClass::Interactive, "update-check", move |context| {
            crate::log_info!("[Updater] check_started task={}", context.id());
            let result = if context.should_stop() {
                Err(anyhow::anyhow!("Update check was cancelled"))
            } else {
                latest_candidate()
            };
            owner.finish_check(&task_operation, result);
        }) {
            Ok(ticket) => replace_ticket(&mut tasks.check, ticket),
            Err(error) => self.owner.finish_check(&operation, Err(error)),
        }
    }

    pub fn perform_update(&self) {
        let Ok(mut tasks) = self.tasks.lock() else {
            return;
        };
        let Some((operation, candidate)) = self.owner.begin_download() else {
            return;
        };
        let owner = Arc::clone(&self.owner);
        let task_operation = operation.clone();
        match crate::task_runtime::spawn(TaskClass::Io, "update-download", move |context| {
            crate::log_info!("[Updater] download_started task={}", context.id());
            let result = if context.should_stop() {
                Err(anyhow::anyhow!("Update download was cancelled"))
            } else {
                download_and_stage(candidate, || context.should_stop())
            };
            owner.finish_download(&task_operation, result);
        }) {
            Ok(ticket) => replace_ticket(&mut tasks.download, ticket),
            Err(error) => self.owner.finish_download(&operation, Err(error)),
        }
    }
}

fn replace_ticket(slot: &mut Option<TaskTicket>, ticket: TaskTicket) {
    if let Some(previous) = slot.replace(ticket) {
        crate::log_info!("[Updater] cancelling_retired task={}", previous.id());
        previous.cancel();
    }
}

impl Drop for Updater {
    fn drop(&mut self) {
        self.owner.close();
        if let Ok(tasks) = self.tasks.get_mut() {
            for ticket in [&tasks.check, &tasks.download].into_iter().flatten() {
                crate::log_info!("[Updater] closing_task task={}", ticket.id());
                ticket.cancel();
            }
        }
    }
}
