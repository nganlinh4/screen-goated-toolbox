use std::io::Write;
use std::process::{Command, Stdio};

use super::state_sync_script;

#[test]
fn state_sync_preserves_text_without_executing_it() {
    // Execute the actual host-generated script as JavaScript; check both data
    // fidelity and absence of evaluation for strings throughout the payload.
    let payload = serde_json::json!({
        "draftText": "${globalThis.executed = true} ${missing}",
        "player": {"recent": [{"text": "` \\\" \n \u{2028} \u{2029} 🐱 </script>"}]},
        "catalogs": {"references": [{"label": "${globalThis.executed = true}.wav"}]},
        "__proto__": {"polluted": true},
    });
    let input = serde_json::json!({
        "payload": payload,
        "script": state_sync_script(&payload.to_string()),
    });
    let probe = r#"
        const vm = require('node:vm');
        let input = '';
        process.stdin.setEncoding('utf8');
        process.stdin.on('data', chunk => input += chunk);
        process.stdin.on('end', () => {
            const {payload, script} = JSON.parse(input);
            const context = {window: {__TTS_SET_STATE__(value) { this.value = value; }}};
            vm.runInNewContext(script, context);
            if (context.executed !== undefined ||
                JSON.stringify(context.window.value) !== JSON.stringify(payload)) process.exit(1);
        });
    "#;
    let mut child = Command::new("node")
        .args(["-e", probe])
        .stdin(Stdio::piped())
        .spawn()
        .expect("Node.js is required for the JavaScript transport test");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}
