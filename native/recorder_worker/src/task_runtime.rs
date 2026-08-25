#[derive(Clone, Copy)]
pub(crate) enum TaskClass {
    Interactive,
}

pub(crate) fn spawn_detached(
    _class: TaskClass,
    name: &'static str,
    work: impl FnOnce() + Send + 'static,
) {
    if let Err(error) = std::thread::Builder::new()
        .name(name.to_string())
        .spawn(work)
    {
        crate::log_info!("[TaskRuntime] detached_not_started name={name} error={error}");
    }
}
