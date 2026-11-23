# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: tests\perf\editor-lag.spec.ts >> synthetic huge editor opens Projects without blocking the UI
- Location: tests\perf\editor-lag.spec.ts:326:1

# Error details

```
Error: expect(received).toBeLessThanOrEqual(expected)

Expected: <= 700
Received:    702.5999999940395
```

# Page snapshot

```yaml
- generic [ref=e3]:
  - banner [ref=e9]:
    - generic [ref=e10]:
      - generic:
        - generic:
          - generic:
            - generic:
              - button "Insert clip at start":
                - img
              - generic:
                - generic:
                  - button "Synthetic Clip 1 Original":
                    - generic: Synthetic Clip 1
                    - generic: Original
              - button "Insert clip at end":
                - img
          - generic:
            - generic:
              - paragraph: Synthetic Clip 1
              - generic:
                - generic:
                  - generic:
                    - img
                  - generic: Raw video
                - generic:
                  - generic:
                    - img
                  - generic: Imported audio
                - generic:
                  - generic:
                    - img
                  - generic: Subtitles
                - generic:
                  - generic:
                    - img
                  - generic: Text overlays
                - generic:
                  - generic:
                    - img
                  - generic: Cursor data
                - generic:
                  - generic:
                    - img
                  - generic: Webcam
                - generic:
                  - generic:
                    - img
                  - generic: Microphone
                - generic:
                  - generic:
                    - img
                  - generic: Device audio
                - generic:
                  - generic:
                    - img
                  - generic: Keystrokes
            - generic:
              - button "Close project":
                - img
                - generic: Close project
    - generic [ref=e11]:
      - button "Add Hotkey" [ref=e13]:
        - img
        - text: Add Hotkey
      - generic [ref=e14]:
        - button "Device Audio" [ref=e16]:
          - img
          - generic [ref=e17]: Device Audio
          - img
        - button "No Cursor" [ref=e19]:
          - generic [ref=e20]: No Cursor
          - img
        - button "Display capture" [ref=e22]:
          - generic [ref=e23]: Display capture
          - img
        - button "Projects" [ref=e24]:
          - img
          - text: Projects
      - generic [ref=e25]:
        - button "Minimize" [ref=e26]:
          - img [ref=e27]
        - button "Maximize" [ref=e29]:
          - img [ref=e30]
        - button "Close" [ref=e32]:
          - img [ref=e33]
  - main [ref=e35]:
    - generic [ref=e43]:
      - tablist "Editor tools" [ref=e44]:
        - tab "Camera" [ref=e45]
        - tab "Background" [selected] [ref=e46]: Background
        - tab "Blur" [ref=e48]
        - tab "Audio" [ref=e49]
        - tab "Narration" [ref=e50]
        - tab "Subtitles" [ref=e51]
      - tabpanel "Background" [ref=e54]:
        - generic [ref=e56]:
          - generic [ref=e57]:
            - generic [ref=e58]: Video Size
            - group "Video Size" [ref=e59]:
              - slider "Video Size" [ref=e60] [cursor=pointer]: "90"
            - generic [ref=e61]: 90%
          - generic [ref=e62]:
            - generic [ref=e63]: Roundness
            - group "Roundness" [ref=e64]:
              - slider "Roundness" [ref=e65] [cursor=pointer]: "32"
            - generic [ref=e66]: 32px
          - generic [ref=e67]:
            - generic [ref=e68]: Shadow
            - group "Shadow" [ref=e69]:
              - slider "Shadow" [ref=e70] [cursor=pointer]: "100"
            - generic [ref=e71]: 100px
          - generic [ref=e72]:
            - generic [ref=e73]: Zoom Background
            - switch "Zoom Background" [checked] [ref=e74]
          - generic [ref=e76]:
            - generic [ref=e77]: Background Style
            - generic [ref=e78]:
              - generic "Upload background" [ref=e79] [cursor=pointer]:
                - img [ref=e81]
              - button [ref=e83]
              - button [ref=e84]
              - button [ref=e85]
              - button [ref=e86]
              - button [ref=e87]
              - button [ref=e88]
              - button [ref=e89]
              - button [ref=e90]
              - button [ref=e91]
              - button [ref=e92]
              - button [ref=e93]
              - button [ref=e94]
              - button [ref=e95]
              - button [ref=e96]
              - button [ref=e97]
              - button [ref=e98]
              - button [ref=e99]
              - button "warm-abstract" [ref=e100]:
                - img "warm-abstract" [ref=e101]
                - img [ref=e103]
              - button "cool-abstract" [ref=e105]:
                - img "cool-abstract" [ref=e106]
                - img [ref=e108]
              - button "deep-abstract" [ref=e110]:
                - img "deep-abstract" [ref=e111]
                - img [ref=e113]
              - button "vivid-abstract" [ref=e115]:
                - img "vivid-abstract" [ref=e116]
                - img [ref=e118]
              - button "macos-tahoe" [ref=e120]:
                - img "macos-tahoe" [ref=e121]
                - img [ref=e123]
              - button "gdrive-2" [ref=e125]:
                - img "gdrive-2" [ref=e126]
                - img [ref=e128]
              - button "gdrive-3" [ref=e130]:
                - img "gdrive-3" [ref=e131]
                - img [ref=e133]
              - button "mojave-dunes" [ref=e135]:
                - img "mojave-dunes" [ref=e136]
                - img [ref=e138]
              - button "catalina" [ref=e140]:
                - img "catalina" [ref=e141]
                - img [ref=e143]
              - button "big-sur" [ref=e145]:
                - img "big-sur" [ref=e146]
                - img [ref=e148]
              - button "el-capitan" [ref=e150]:
                - img "el-capitan" [ref=e151]
                - img [ref=e153]
              - button "beach-aerial" [ref=e155]:
                - img "beach-aerial" [ref=e156]
                - img [ref=e158]
              - button "sierra-sunset" [ref=e160]:
                - img "sierra-sunset" [ref=e161]
                - img [ref=e163]
              - button "windows-11-3d" [ref=e165]:
                - img "windows-11-3d" [ref=e166]
                - img [ref=e168]
              - button "cerro-torre" [ref=e170]:
                - img "cerro-torre" [ref=e171]
                - img [ref=e173]
              - button "ipados-orange" [ref=e175]:
                - img "ipados-orange" [ref=e176]
                - img [ref=e178]
              - button "ipados-blue" [ref=e180]:
                - img "ipados-blue" [ref=e181]
                - img [ref=e183]
              - button "blue-waves" [ref=e185]:
                - img "blue-waves" [ref=e186]
                - img [ref=e188]
              - button "windows-xp" [ref=e190]:
                - img "windows-xp" [ref=e191]
                - img [ref=e193]
              - button "antelope-canyon" [ref=e195]:
                - img "antelope-canyon" [ref=e196]
                - img [ref=e198]
              - button "windows-7" [ref=e200]:
                - img "windows-7" [ref=e201]
                - img [ref=e203]
              - button "windows-11-colorful" [ref=e205]:
                - img "windows-11-colorful" [ref=e206]
                - img [ref=e208]
              - button "big-sur-iridescence" [ref=e210]:
                - img "big-sur-iridescence" [ref=e211]
                - img [ref=e213]
              - button "landscape-rocks" [ref=e215]:
                - img "landscape-rocks" [ref=e216]
                - img [ref=e218]
              - button "lake-mountains" [ref=e220]:
                - img "lake-mountains" [ref=e221]
                - img [ref=e223]
              - button "big-sur-rocks" [ref=e225]:
                - img "big-sur-rocks" [ref=e226]
                - img [ref=e228]
              - button "big-sur-waves" [ref=e230]:
                - img "big-sur-waves" [ref=e231]
                - img [ref=e233]
              - button "sierra-glacier" [ref=e235]:
                - img "sierra-glacier" [ref=e236]
                - img [ref=e238]
              - button "monterey-dark" [ref=e240]:
                - img "monterey-dark" [ref=e241]
                - img [ref=e243]
    - generic [ref=e248]:
      - generic [ref=e249]:
        - generic [ref=e250]:
          - generic [ref=e251]:
            - generic [ref=e252]: Zoom
            - button "D" [ref=e253]
          - generic [ref=e254]:
            - generic [ref=e255]: Speed
            - button "R" [ref=e256]
          - generic [ref=e257]:
            - generic [ref=e258]: Audio
            - 'button "Download audio track: Audio" [ref=e259]':
              - img [ref=e260]
            - button "Add audio file" [ref=e262]:
              - img [ref=e263]
          - generic [ref=e265]:
            - generic [ref=e266]: Device Audio
            - 'button "Download audio track: Device Audio" [ref=e267]':
              - img [ref=e268]
            - generic:
              - generic:
                - generic:
                  - slider "Device Audio track delay": "0"
                - generic: 0.00s
          - generic [ref=e270]:
            - generic [ref=e271]: Webcam
            - generic:
              - generic:
                - generic:
                  - slider "Webcam track delay": "0"
                - generic: 0.00s
          - generic [ref=e272]:
            - generic [ref=e273]: Narration
            - 'button "Download audio track: Narration" [ref=e274]':
              - img [ref=e275]
          - generic [ref=e277]:
            - generic [ref=e278]: Subtitles
            - button "Import subtitles (SRT/VTT)" [ref=e279]:
              - img [ref=e280]
          - generic [ref=e283]: Text
          - generic [ref=e285]: Video
        - generic "Volume" [ref=e287]:
          - img [ref=e288]
          - switch "Volume" [ref=e290]
      - generic [ref=e294]:
        - generic [ref=e296]:
          - img [ref=e300]
          - generic [ref=e308]:
            - generic [ref=e312]:
              - generic: Audio 1
              - generic [ref=e313]: 1.25×
            - generic [ref=e318]: Audio 2
            - generic [ref=e323]: Audio 3
            - generic [ref=e328]: Audio 4
            - generic [ref=e332]:
              - generic: Audio 5
              - generic [ref=e333]: 1.25×
            - generic [ref=e338]: Audio 6
            - generic [ref=e343]: Audio 7
            - generic [ref=e348]: Audio 8
            - generic [ref=e352]:
              - generic: Audio 9
              - generic [ref=e353]: 1.25×
            - generic [ref=e358]: Audio 10
            - generic [ref=e363]: Audio 11
            - generic [ref=e368]: Audio 12
            - generic [ref=e372]:
              - generic: Audio 13
              - generic [ref=e373]: 1.25×
            - generic [ref=e378]: Audio 14
            - generic [ref=e383]: Audio 15
            - generic [ref=e388]: Audio 16
            - generic [ref=e392]:
              - generic: Audio 17
              - generic [ref=e393]: 1.25×
            - generic [ref=e398]: Audio 18
            - generic [ref=e403]: Audio 19
            - generic [ref=e408]: Audio 20
            - generic [ref=e412]:
              - generic: Audio 21
              - generic [ref=e413]: 1.25×
            - generic [ref=e418]: Audio 22
            - generic [ref=e423]: Audio 23
            - generic [ref=e428]: Audio 24
            - generic [ref=e432]:
              - generic: Audio 25
              - generic [ref=e433]: 1.25×
            - generic [ref=e438]: Audio 26
            - generic [ref=e443]: Audio 27
            - generic [ref=e448]: Audio 28
            - generic [ref=e452]:
              - generic: Audio 29
              - generic [ref=e453]: 1.25×
            - generic [ref=e458]: Audio 30
            - generic [ref=e463]: Audio 31
            - generic [ref=e468]: Audio 32
            - generic [ref=e472]:
              - generic: Audio 33
              - generic [ref=e473]: 1.25×
            - generic [ref=e478]: Audio 34
            - generic [ref=e483]: Audio 35
            - generic [ref=e488]: Audio 36
            - generic [ref=e492]:
              - generic: Audio 37
              - generic [ref=e493]: 1.25×
            - generic [ref=e498]: Audio 38
            - generic [ref=e503]: Audio 39
            - generic [ref=e508]: Audio 40
            - generic [ref=e512]:
              - generic: Audio 41
              - generic [ref=e513]: 1.25×
            - generic [ref=e518]: Audio 42
            - generic [ref=e523]: Audio 43
            - generic [ref=e528]: Audio 44
            - generic [ref=e532]:
              - generic: Audio 45
              - generic [ref=e533]: 1.25×
            - generic [ref=e538]: Audio 46
            - generic [ref=e543]: Audio 47
            - generic [ref=e548]: Audio 48
            - generic [ref=e552]:
              - generic: Audio 49
              - generic [ref=e553]: 1.25×
            - generic [ref=e558]: Audio 50
            - generic [ref=e563]: Audio 51
            - generic [ref=e568]: Audio 52
            - generic [ref=e572]:
              - generic: Audio 53
              - generic [ref=e573]: 1.25×
            - generic [ref=e578]: Audio 54
            - generic [ref=e583]: Audio 55
            - generic [ref=e588]: Audio 56
            - generic [ref=e592]:
              - generic: Audio 57
              - generic [ref=e593]: 1.25×
            - generic [ref=e598]: Audio 58
            - generic [ref=e603]: Audio 59
            - generic [ref=e608]: Audio 60
            - generic [ref=e612]:
              - generic: Audio 61
              - generic [ref=e613]: 1.25×
            - generic [ref=e618]: Audio 62
            - generic [ref=e623]: Audio 63
            - generic [ref=e628]: Audio 64
            - generic [ref=e632]:
              - generic: Audio 65
              - generic [ref=e633]: 1.25×
            - generic [ref=e638]: Audio 66
            - generic [ref=e643]: Audio 67
            - generic [ref=e648]: Audio 68
            - generic [ref=e652]:
              - generic: Audio 69
              - generic [ref=e653]: 1.25×
            - generic [ref=e658]: Audio 70
            - generic [ref=e663]: Audio 71
            - generic [ref=e668]: Audio 72
            - generic [ref=e672]:
              - generic: Audio 73
              - generic [ref=e673]: 1.25×
            - generic [ref=e678]: Audio 74
            - generic [ref=e683]: Audio 75
            - generic [ref=e688]: Audio 76
            - generic [ref=e692]:
              - generic: Audio 77
              - generic [ref=e693]: 1.25×
            - generic [ref=e698]: Audio 78
            - generic [ref=e703]: Audio 79
            - generic [ref=e708]: Audio 80
          - generic [ref=e719]: ●
          - generic [ref=e728]:
            - generic: Clip 1 overlay
        - generic [ref=e741]:
          - generic [ref=e744]: 0:00
          - generic [ref=e747]: 0:58
          - generic [ref=e750]: 1:47
          - generic [ref=e753]: 2:32
          - generic [ref=e756]: 3:24
          - generic [ref=e759]: 4:33
          - generic [ref=e762]: 5:41
          - generic [ref=e765]: 6:27
          - generic [ref=e768]: 7:05
          - generic [ref=e771]: 7:42
          - generic [ref=e774]: 8:20
  - generic [ref=e779]:
    - generic [ref=e780]:
      - generic [ref=e781]:
        - heading "Projects" [level=3] [ref=e782]
        - generic [ref=e783] [cursor=pointer]:
          - img [ref=e784]
          - text: Import Video (Or drag and drop video here)
        - generic [ref=e786] [cursor=pointer]:
          - img [ref=e787]
          - text: Import Audio (Or drag and drop audio here)
      - generic [ref=e789]:
        - generic [ref=e790]: Max
        - slider "Max" [ref=e791] [cursor=pointer]: "50"
        - generic [ref=e792]: "50"
        - button "Close" [ref=e793]:
          - img [ref=e794]
    - generic [ref=e797]: No projects yet
```

# Test source

```ts
  240 |     budgets.syntheticHuge.wheelMaxLongTaskMs,
  241 |   );
  242 |   expect(finalDom?.totalTimelineBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.totalTimelineBlocks);
  243 | });
  244 | 
  245 | test("synthetic huge audio panel speed and trim scrubs stay responsive", async ({ page }) => {
  246 |   await loadHugeFixture(page);
  247 |   expect(await page.evaluate(() => window.__SGT_EDITOR_TEST__?.selectFirstAudioSegment())).toBe(true);
  248 |   await expect(page.locator(".audio-panel-single-editor")).toBeVisible();
  249 | 
  250 |   await page.evaluate(() => window.__SGT_TEST__?.resetPerf());
  251 |   await page.evaluate(() => window.__SGT_TEST__?.startPerfProbe());
  252 | 
  253 |   await page.evaluate(() => window.__SGT_TEST__?.startAction("scrub:audio-speed"));
  254 |   const speedMs = await scrubRangeAndMeasure(
  255 |     page,
  256 |     ".audio-panel-speed-slider",
  257 |     Array.from({ length: 40 }, (_, index) => 1 + index * 0.025),
  258 |   );
  259 |   await page.evaluate(() => window.__SGT_TEST__?.endAction("scrub:audio-speed"));
  260 | 
  261 |   await page.evaluate(() => window.__SGT_TEST__?.startAction("scrub:audio-trim"));
  262 |   const trimMs = await scrubRangeAndMeasure(
  263 |     page,
  264 |     ".audio-panel-trim-start-slider",
  265 |     Array.from({ length: 40 }, (_, index) => index * 0.015),
  266 |   );
  267 |   await page.evaluate(() => window.__SGT_TEST__?.endAction("scrub:audio-trim"));
  268 | 
  269 |   await page.waitForTimeout(120);
  270 |   const frameProbe = await page.evaluate(() => window.__SGT_TEST__?.stopPerfProbe());
  271 |   const perf = await page.evaluate(() => window.__SGT_TEST__?.getPerfSnapshot());
  272 | 
  273 |   expect(speedMs).toBeLessThanOrEqual(budgets.syntheticHuge.audioPanelSpeedScrubMs);
  274 |   expect(trimMs).toBeLessThanOrEqual(budgets.syntheticHuge.audioPanelTrimScrubMs);
  275 |   expect(frameProbe?.p95FrameDeltaMs ?? 0).toBeLessThanOrEqual(
  276 |     budgets.syntheticHuge.audioPanelControlP95FrameDeltaMs,
  277 |   );
  278 |   expect(Math.max(0, ...(perf?.longTasks.map((entry) => entry.duration) ?? []))).toBeLessThanOrEqual(
  279 |     budgets.syntheticHuge.audioPanelControlMaxLongTaskMs,
  280 |   );
  281 | });
  282 | 
  283 | test("synthetic huge editor fixture stays within attributed lag budget", async ({ page }) => {
  284 |   const { loadProjectMs } = await loadHugeFixture(page);
  285 | 
  286 |   const initialDom = await page.evaluate(() => window.__SGT_TEST__?.getDomStats());
  287 |   expect(initialDom?.subtitleBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.subtitleDomBlocks);
  288 |   expect(initialDom?.audioBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.audioDomBlocks);
  289 |   expect(initialDom?.narrationBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.narrationDomBlocks);
  290 |   expect(initialDom?.totalTimelineBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.totalTimelineBlocks);
  291 | 
  292 |   await page.evaluate(() => window.__SGT_TEST__?.startPerfProbe());
  293 | 
  294 |   await page.evaluate(() => window.__SGT_TEST__?.startAction("wheel-zoom"));
  295 |   await wheelZoom(page);
  296 |   await page.evaluate(() => window.__SGT_TEST__?.endAction("wheel-zoom"));
  297 | 
  298 |   await page.evaluate(() => window.__SGT_TEST__?.startAction("drag:subtitle"));
  299 |   await dragLane(page, ".subtitle-track");
  300 |   await page.evaluate(() => window.__SGT_TEST__?.endAction("drag:subtitle"));
  301 | 
  302 |   await page.evaluate(() => window.__SGT_TEST__?.startAction("drag:audio"));
  303 |   await dragLane(page, ".audio-track");
  304 |   await page.evaluate(() => window.__SGT_TEST__?.endAction("drag:audio"));
  305 | 
  306 |   await page.evaluate(() => window.__SGT_TEST__?.startAction("drag:narration"));
  307 |   await dragLane(page, ".narration-track");
  308 |   await page.evaluate(() => window.__SGT_TEST__?.endAction("drag:narration"));
  309 | 
  310 |   await page.waitForTimeout(300);
  311 |   const frameProbe = await page.evaluate(() => window.__SGT_TEST__?.stopPerfProbe());
  312 |   const perf = await page.evaluate(() => window.__SGT_TEST__?.getPerfSnapshot());
  313 |   const finalDom = await page.evaluate(() => window.__SGT_TEST__?.getDomStats());
  314 | 
  315 |   expect(loadProjectMs).toBeLessThanOrEqual(budgets.syntheticHuge.loadProjectMs);
  316 |   expect(perf?.events.some((entry) => entry.label === "drag:subtitle" && typeof entry.duration === "number")).toBe(true);
  317 |   expect(perf?.renderCounters?.SubtitleTrack ?? 0).toBeGreaterThan(0);
  318 |   expect(frameProbe?.p95FrameDeltaMs ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.dragP95FrameDeltaMs);
  319 |   expect(Math.max(0, ...(perf?.longTasks.map((entry) => entry.duration) ?? []))).toBeLessThanOrEqual(
  320 |     budgets.syntheticHuge.maxLongTaskMs,
  321 |   );
  322 |   expect(perf?.longTasks.length ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.longTaskCount);
  323 |   expect(finalDom?.totalTimelineBlocks ?? 0).toBeLessThanOrEqual(budgets.syntheticHuge.totalTimelineBlocks);
  324 | });
  325 | 
  326 | test("synthetic huge editor opens Projects without blocking the UI", async ({ page }) => {
  327 |   await loadHugeFixture(page);
  328 |   await page.evaluate(() => window.__SGT_TEST__?.resetPerf());
  329 |   await page.evaluate(() => window.__SGT_TEST__?.startPerfProbe());
  330 | 
  331 |   const openMs = await openProjectsAndMeasure(page, budgets.syntheticHuge.projectsOpenMs);
  332 |   await expect(page.locator(".projects-view")).toBeVisible({
  333 |     timeout: budgets.syntheticHuge.projectsOpenMs,
  334 |   });
  335 | 
  336 |   await page.waitForTimeout(120);
  337 |   await page.evaluate(() => window.__SGT_TEST__?.stopPerfProbe());
  338 |   const perf = await page.evaluate(() => window.__SGT_TEST__?.getPerfSnapshot());
  339 | 
> 340 |   expect(openMs).toBeLessThanOrEqual(budgets.syntheticHuge.projectsOpenMs);
      |                  ^ Error: expect(received).toBeLessThanOrEqual(expected)
  341 |   expect(Math.max(0, ...(perf?.longTasks.map((entry) => entry.duration) ?? []))).toBeLessThanOrEqual(
  342 |     budgets.syntheticHuge.projectsOpenMaxLongTaskMs,
  343 |   );
  344 | });
  345 | 
```