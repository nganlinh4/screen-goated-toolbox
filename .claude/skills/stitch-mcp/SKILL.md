# Stitch MCP — Google AI UI Design Tool

## What it does
Stitch (stitch.withgoogle.com) generates production-quality UI designs from text prompts. It outputs HTML/CSS code and screenshots. Use it when you need a polished UI design reference for any mini app or overlay.

## Setup (already done)

### Global npm package
```bash
npm install -g @_davideast/stitch-mcp
```

### OAuth credentials (gcloud)
Credentials are stored at `C:\Users\user\.stitch-mcp\config\`.
- Authenticated as `nganlinh4@gmail.com`
- Google Cloud project: `potent-catwalk-461107-m8`
- Stitch API enabled on this project

To refresh auth if it expires:
```powershell
$env:CLOUDSDK_CONFIG = "C:\Users\user\.stitch-mcp\config"
& "C:\Users\user\.stitch-mcp\google-cloud-sdk\bin\gcloud.cmd" auth application-default login
```

### Wrapper script
`C:\Users\user\.claude\stitch-mcp-wrapper.mjs` solves two bugs in stitch-mcp:

1. **Timeout fix**: stitch-mcp takes ~5s to connect to Google API before starting the MCP protocol. The wrapper monkey-patches `StitchProxy.prototype.start` to connect the MCP stdio transport immediately, then connects to Stitch in the background.

2. **OAuth fix**: stitch-mcp proxy hardcodes `X-Goog-Api-Key` headers but the Stitch API rejects API keys for all operations. The wrapper intercepts `globalThis.fetch` and replaces API key headers with OAuth Bearer tokens obtained from gcloud ADC.

The SDK file at `npm/node_modules/@_davideast/stitch-mcp/dist/chunk-0cd2xak4.js` is also patched (with `.bak` backup) to use `STITCH_ACCESS_TOKEN` env when available.

### MCP config (in `~/.claude.json` under the project)
```json
{
  "stitch": {
    "type": "stdio",
    "command": "node",
    "args": ["C:\\Users\\user\\.claude\\stitch-mcp-wrapper.mjs"],
    "env": {
      "STITCH_API_KEY": "AIzaSyAd8-GvAh3RaDl-jecnZm3P3a8677YkHu4",
      "GOOGLE_CLOUD_PROJECT": "potent-catwalk-461107-m8",
      "CLOUDSDK_CONFIG": "C:\\Users\\user\\.stitch-mcp\\config",
      "GOOGLE_APPLICATION_CREDENTIALS": "C:\\Users\\user\\.stitch-mcp\\config\\application_default_credentials.json"
    }
  }
}
```

**Important**: `STITCH_API_KEY` is required for the proxy startup validation but NOT used for actual API calls (the wrapper replaces it with OAuth Bearer tokens at runtime).

## Usage workflow

### 1. Create a project
```
mcp__stitch__create_project(title: "My UI Design")
```
Returns a project ID like `1541934746488160353`.

### 2. Generate a screen
```
mcp__stitch__generate_screen_from_text(
  projectId: "1541934746488160353",
  prompt: "A dark-themed settings panel with...",
  deviceType: "DESKTOP",  // or MOBILE, TABLET, AGNOSTIC
  modelId: "GEMINI_3_1_PRO"  // best quality
)
```
This takes 1-2 minutes. DO NOT RETRY if it seems slow.

Returns:
- `htmlCode.downloadUrl` — full HTML file with Tailwind CSS
- `screenshot.downloadUrl` — PNG preview
- `designSystem` — generated design tokens and guidelines
- `suggestion` entries — follow-up ideas

### 3. Download the HTML
```bash
curl -sL "<downloadUrl>" -o stitch-design.html
```

### 4. Iterate
Use `mcp__stitch__edit_screens` to refine, or generate variants with `mcp__stitch__generate_variants`.

## Troubleshooting

### "Failed to connect" on restart
The wrapper needs the npm package cached. Run once manually to warm cache:
```bash
STITCH_API_KEY=AIzaSyAd8-GvAh3RaDl-jecnZm3P3a8677YkHu4 stitch-mcp proxy
```

### "API keys are not supported" (401)
The OAuth token expired. Refresh:
```powershell
$env:CLOUDSDK_CONFIG = "C:\Users\user\.stitch-mcp\config"
& "C:\Users\user\.stitch-mcp\google-cloud-sdk\bin\gcloud.cmd" auth application-default login
```

### Tools load but don't appear in session
Sometimes Stitch MCP connects but tools aren't listed. Restart Claude Code — they usually appear on second try.

### npm update breaks the SDK patch
If `npm update -g @_davideast/stitch-mcp` runs, the `chunk-0cd2xak4.js` patch will be lost. Restore from `.bak` or re-apply the sed patches:
```bash
SDKFILE="$APPDATA/npm/node_modules/@_davideast/stitch-mcp/dist/chunk-0cd2xak4.js"
# Patch all X-Goog-Api-Key headers to use OAuth when STITCH_ACCESS_TOKEN is set
sed -i 's|"X-Goog-Api-Key": config3.apiKey|...(process.env.STITCH_ACCESS_TOKEN ? {"Authorization": "Bearer " + process.env.STITCH_ACCESS_TOKEN, "X-Goog-User-Project": process.env.GOOGLE_CLOUD_PROJECT} : {"X-Goog-Api-Key": config3.apiKey})|g' "$SDKFILE"
sed -i 's|"X-Goog-Api-Key": ctx.config.apiKey|...(process.env.STITCH_ACCESS_TOKEN ? {"Authorization": "Bearer " + process.env.STITCH_ACCESS_TOKEN, "X-Goog-User-Project": process.env.GOOGLE_CLOUD_PROJECT} : {"X-Goog-Api-Key": ctx.config.apiKey})|g' "$SDKFILE"
```
