//! Curated catalog of MCP app-control integrations. Data-driven and PINNED;
//! NOTHING here is ever built from model input. Adding an entry is the only way the
//! agent can gain a new integration — there is no arbitrary-registry crawl.

/// How to launch an integration's MCP server over stdio.
pub(super) struct LaunchSpec {
    pub program: &'static str,
    pub args: &'static [&'static str],
    pub env: &'static [(&'static str, &'static str)],
}

/// A safe, declarative app-side readiness check. This is "what proves ready",
/// not a setup recipe.
#[derive(Clone, Copy)]
pub(super) enum ReadinessProbe {
    Tcp { host: &'static str, port: u16 },
}

/// One curated integration.
pub(super) struct Integration {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub publisher: &'static str,
    pub source_url: &'static str,
    pub launch: LaunchSpec,
    /// One-line statement of WHAT in-app setup the integration needs (never HOW — the
    /// agent researches the steps from `source_url` + the web and figures them out).
    /// `None` = fully self-contained, no in-app setup.
    pub addon_hint: Option<&'static str>,
    pub readiness_probe: Option<ReadinessProbe>,
    /// Exact zero-argument tool that this pinned integration authorizes for a
    /// semantic readiness call when the server does not publish read-only MCP
    /// annotations. This is protocol metadata, never inferred from prose.
    pub semantic_probe_tool: Option<&'static str>,
}

const CATALOG: &[Integration] = &[
    // Self-contained reference server (uvx fetches it on demand). Verifies the bridge.
    Integration {
        id: "time",
        display_name: "Time",
        description: "Current time + timezone conversion (IANA zones). Reference integration that verifies the MCP bridge.",
        publisher: "modelcontextprotocol.io (official)",
        source_url: "https://github.com/modelcontextprotocol/servers",
        launch: LaunchSpec {
            program: "uvx",
            args: &["--from", "mcp-server-time==2026.6.4", "mcp-server-time"],
            env: &[],
        },
        addon_hint: None,
        readiness_probe: None,
        semantic_probe_tool: None,
    },
    // App-control via an in-Blender add-on (socket on localhost:9876) + a uvx bridge.
    Integration {
        id: "blender",
        display_name: "Blender",
        description: "Drive Blender's Python API directly (objects, materials, render, export, …) instead of clicking its UI.",
        publisher: "ahujasid (blender-mcp)",
        source_url: "https://github.com/ahujasid/blender-mcp",
        launch: LaunchSpec {
            program: "uvx",
            args: &[
                "--python",
                "3.11",
                "--python-preference",
                "only-managed",
                "--from",
                "blender-mcp==1.6.4",
                "blender-mcp",
            ],
            env: &[
                ("DISABLE_TELEMETRY", "true"),
                ("UV_PYTHON_PREFERENCE", "only-managed"),
            ],
        },
        addon_hint: Some(
            "Blender needs its MCP add-on installed + enabled and its socket server started, inside the running Blender.",
        ),
        readiness_probe: Some(ReadinessProbe::Tcp {
            host: "127.0.0.1",
            port: 9876,
        }),
        semantic_probe_tool: Some("get_scene_info"),
    },
];

pub(super) fn all() -> &'static [Integration] {
    CATALOG
}

pub(super) fn get(id: &str) -> Option<&'static Integration> {
    CATALOG.iter().find(|i| i.id == id)
}
