use super::super::prose::bounded_prose;
use super::{McpTool, McpToolAnnotations};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashSet;

const MAX_TOOL_LIST_PAGES: usize = 1024;

pub(super) fn collect_tool_pages(
    mut request_page: impl FnMut(Value) -> Result<Value>,
) -> Result<Vec<McpTool>> {
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors = HashSet::new();
    let mut seen_tools = HashSet::new();

    for _ in 0..MAX_TOOL_LIST_PAGES {
        let params = cursor
            .as_ref()
            .map_or_else(|| json!({}), |cursor| json!({"cursor": cursor}));
        let result = request_page(params)?;
        let page_tools = result
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("mcp tools/list page is missing its tools array"))?;
        for (index, tool) in page_tools.iter().enumerate() {
            let parsed = parse_tool(tool)
                .map_err(|error| anyhow!("mcp tools/list tool {index} is malformed: {error}"))?;
            if !seen_tools.insert(parsed.name.clone()) {
                bail!(
                    "mcp tools/list returned duplicate tool name '{}'",
                    parsed.name
                );
            }
            tools.push(parsed);
        }

        let Some(next_cursor) = result.get("nextCursor") else {
            return Ok(tools);
        };
        if next_cursor.is_null() {
            return Ok(tools);
        }
        let next_cursor = next_cursor
            .as_str()
            .ok_or_else(|| anyhow!("mcp tools/list returned a non-string nextCursor"))?
            .to_string();
        if !seen_cursors.insert(next_cursor.clone()) {
            bail!("mcp tools/list cursor cycle detected");
        }
        cursor = Some(next_cursor);
    }

    bail!("mcp tools/list exceeded the bounded pagination limit of {MAX_TOOL_LIST_PAGES} pages")
}

fn parse_tool(value: &Value) -> Result<McpTool> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("tool entry is not an object"))?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("tool name is missing or empty"))?;
    let description = match object.get("description") {
        Some(description) => description
            .as_str()
            .ok_or_else(|| anyhow!("tool description is not a string"))?,
        None => "",
    };
    let input_schema = object
        .get("inputSchema")
        .filter(|schema| schema.is_object())
        .cloned()
        .ok_or_else(|| anyhow!("tool inputSchema is missing or not an object"))?;
    if object
        .get("annotations")
        .is_some_and(|annotations| !annotations.is_object())
    {
        bail!("tool annotations is not an object");
    }
    Ok(McpTool {
        name: name.to_string(),
        description: bounded_prose(description),
        input_schema,
        annotations: parse_annotations(object.get("annotations")),
    })
}

fn parse_annotations(value: Option<&Value>) -> McpToolAnnotations {
    let boolean = |key| value?.get(key)?.as_bool();
    McpToolAnnotations {
        read_only: boolean("readOnlyHint"),
        destructive: boolean("destructiveHint"),
        open_world: boolean("openWorldHint"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn parses_only_boolean_tool_annotations() {
        assert_eq!(
            parse_annotations(Some(&json!({
                "readOnlyHint": true,
                "destructiveHint": false,
                "openWorldHint": "false"
            }))),
            McpToolAnnotations {
                read_only: Some(true),
                destructive: Some(false),
                open_world: None,
            }
        );
        assert_eq!(parse_annotations(None), McpToolAnnotations::default());
    }

    #[test]
    fn tool_listing_follows_every_cursor_and_preserves_page_order() {
        let mut pages = VecDeque::from([
            json!({
                "tools": [{"name": "first", "description": "one", "inputSchema": {"type": "object"}}],
                "nextCursor": "page-2"
            }),
            json!({
                "tools": [{"name": "second", "description": "two", "inputSchema": {}}],
                "nextCursor": "page-3"
            }),
            json!({"tools": [{"name": "third", "description": "three", "inputSchema": {}}]}),
        ]);
        let mut params = Vec::new();

        let tools = collect_tool_pages(|request| {
            params.push(request);
            pages
                .pop_front()
                .ok_or_else(|| anyhow!("unexpected extra page request"))
        })
        .expect("all pages should be collected");

        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        assert_eq!(
            params,
            [
                json!({}),
                json!({"cursor": "page-2"}),
                json!({"cursor": "page-3"})
            ]
        );
        assert!(pages.is_empty());
    }

    #[test]
    fn tool_listing_treats_null_cursor_as_completion() {
        let mut calls = 0;
        let tools = collect_tool_pages(|_| {
            calls += 1;
            Ok(json!({"tools": [{"name": "only", "inputSchema": {}}], "nextCursor": null}))
        })
        .expect("null cursor should end pagination");

        assert_eq!(calls, 1);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn tool_listing_rejects_cursor_cycles() {
        let mut pages = VecDeque::from([
            json!({"tools": [], "nextCursor": "again"}),
            json!({"tools": [], "nextCursor": "again"}),
        ]);

        let result = collect_tool_pages(|_| {
            pages
                .pop_front()
                .ok_or_else(|| anyhow!("unexpected extra page request"))
        });
        let error = result.err().expect("repeated cursor must not loop forever");

        assert!(error.to_string().contains("cursor cycle"));
    }

    #[test]
    fn tool_listing_rejects_malformed_cursor() {
        let error = collect_tool_pages(|_| Ok(json!({"tools": [], "nextCursor": 7})))
            .err()
            .expect("cursor must be an opaque string");

        assert!(error.to_string().contains("non-string nextCursor"));
    }

    #[test]
    fn tool_listing_requires_an_array_on_every_page() {
        let error = collect_tool_pages(|_| Ok(json!({"tools": {}})))
            .err()
            .expect("a malformed page must invalidate the complete catalog");

        assert!(error.to_string().contains("tools array"));
    }

    #[test]
    fn tool_listing_rejects_a_malformed_entry_instead_of_returning_a_partial_catalog() {
        let error = collect_tool_pages(|_| {
            Ok(json!({
                "tools": [
                    {"name": "valid", "inputSchema": {}},
                    {"name": "missing-schema"}
                ]
            }))
        })
        .err()
        .expect("one malformed entry must invalidate the complete catalog");

        assert!(error.to_string().contains("tool 1 is malformed"));
        assert!(error.to_string().contains("inputSchema"));
    }

    #[test]
    fn tool_listing_rejects_duplicate_names_across_pages() {
        let mut pages = VecDeque::from([
            json!({"tools": [{"name": "same", "inputSchema": {}}], "nextCursor": "next"}),
            json!({"tools": [{"name": "same", "inputSchema": {}}]}),
        ]);
        let error = collect_tool_pages(|_| {
            pages
                .pop_front()
                .ok_or_else(|| anyhow!("unexpected extra page request"))
        })
        .err()
        .expect("duplicate routes must invalidate the complete catalog");

        assert!(error.to_string().contains("duplicate tool name"));
    }

    #[test]
    fn tool_listing_preserves_an_opaque_valid_input_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "future": {
                    "type": ["string", "null"],
                    "vendorExtension": {"nested": [1, true, null]}
                }
            },
            "unknownTopLevelKeyword": "keep-me"
        });
        let tools = collect_tool_pages(|_| {
            Ok(json!({"tools": [{"name": "future", "inputSchema": schema.clone()}]}))
        })
        .expect("opaque object schemas are valid transport data");

        assert_eq!(tools[0].input_schema, schema);
    }
}
