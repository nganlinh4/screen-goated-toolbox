//! History commands shared by every creation WebView.

use super::*;

/// How a tool keeps its in-memory job results aligned with history changes.
pub(crate) struct ResultPathHooks {
    pub(crate) remap: fn(previous: &str, current: &str),
    pub(crate) forget: fn(path: &str),
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str, CreationError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required").into())
}

fn listed(tool: &str, id: &str) -> Result<ResultHistoryEntry, CreationError> {
    list(tool)?
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| {
            CreationError::new(ErrorCode::ResultMissing, "Result is no longer in history.")
        })
}

fn to_value(value: impl Serialize) -> Result<Value, CreationError> {
    serde_json::to_value(value).map_err(|error| error.to_string().into())
}

fn rename_command(
    tool: &str,
    args: &Value,
    hooks: &ResultPathHooks,
) -> Result<Value, CreationError> {
    let id = required(args, "id")?;
    let new_name = required(args, "newName")?;
    let previous = listed(tool, id)?;
    let updated = rename(tool, id, new_name)?;
    (hooks.remap)(&previous.output_path, &updated.output_path);
    to_value(public_entry(&updated))
}

fn delete_command(
    tool: &str,
    args: &Value,
    hooks: &ResultPathHooks,
) -> Result<Value, CreationError> {
    let id = required(args, "id")?;
    let previous = listed(tool, id)?;
    delete(tool, id)?;
    (hooks.forget)(&previous.output_path);
    Ok(Value::Null)
}

fn delete_all_command(tool: &str, hooks: &ResultPathHooks) -> Result<Value, CreationError> {
    let previous = list(tool)?;
    let deleted = delete_all(tool)?;
    for entry in previous {
        (hooks.forget)(&entry.output_path);
    }
    Ok(Value::from(deleted as u64))
}

/// Returns `None` when `cmd` is not a history command.
pub(crate) fn dispatch(
    tool: &str,
    cmd: &str,
    args: &Value,
    hooks: &ResultPathHooks,
) -> Option<Result<Value, CreationError>> {
    Some(match cmd {
        "history_results" => list(tool)
            .map_err(CreationError::from)
            .and_then(|entries| to_value(public_entries(&entries))),
        "rename_history_result" => rename_command(tool, args, hooks),
        "delete_history_result" => delete_command(tool, args, hooks),
        "delete_all_history_results" => delete_all_command(tool, hooks),
        _ => return None,
    })
}
