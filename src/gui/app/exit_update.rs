pub(super) fn perform_update(
    staged: &crate::updater::StagedUpdate,
    prepared: super::exit_preparation::ExitPreparation,
) -> anyhow::Result<()> {
    staged.with_verified_path_beside(&std::env::current_exe()?, |path| {
        let mut command =
            crate::runtime_support::restart_command(path, std::iter::empty::<&str>())?;
        super::exit_requests::reserve_explicit_exit(|| command.spawn())?;
        Ok(())
    })?;
    super::exit_prepared(prepared)
}
