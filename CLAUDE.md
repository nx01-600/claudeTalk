# claudeTalk: notes for Claude Code

Voice for Claude Code on Windows: talk mode (Claude speaks), local dictation, a
wake phrase, and a glass overlay. Since v0.7.0 it is two native Rust programs;
there is no Python or PowerShell left.

## Layout
- `bin\claudetalk.exe`: hooks (`hook prompt|stop|session-start`), the `say` MCP
  server (`mcp`), `/talk` (`toggle`), the speech player (`speaker`) and
  `cleanup`.
- `claudetalk-dictation.exe`: the dictation daemon (tray, overlay, Whisper,
  wake phrase). It is ~60 MB, so it is **not in git**:
  - `claudetalk.exe` downloads it from the GitHub release `v<its version>` into
    `%LOCALAPPDATA%\claudeTalk\bin\<version>\` (`fetch-dictation`, which the
    SessionStart hook also runs).
  - A copy in `bin\`, next to `claudetalk.exe`, takes precedence (local
    builds; git-ignored).
  - The daemon finds `claudetalk.exe` through
    `%APPDATA%\claudeTalk\claudetalk-path.txt`.
- `native\`: the Rust sources.
  - `native\SPEC.md` is the behavior contract. Check it before changing
    behavior, and list intentional changes in its last section.

## Build
```
native\build.cmd . cargo test --workspace
native\build.cmd . cargo build --release --workspace
```
- `build.cmd` sets up MSVC + Ninja + LLVM + the Vulkan SDK and writes to
  `C:\ctb`. MSBuild fails on long paths in whisper.cpp's shader build.
- Copy `C:\ctb\release\claudetalk*.exe` into `bin\`. Only `claudetalk.exe` is
  committed.

## Releasing
1. Bump the version in `native/Cargo.toml` and in both plugin manifests.
2. Commit and push.
3. Tag `v<version>` and push the tag.
4. CI attaches both exes to that release.

`claudetalk.exe` downloads the daemon of exactly its own version. A version
without a release asset therefore has no dictation, until you upload one
(`gh release upload`).
- `claudetalk-dictation.exe --render-test DIR 1.5` draws the pill and the
  panels to PNGs. The real windows are excluded from screen capture.

## Deploying to the installed plugin
Claude Code runs the plugin from `~\.claude\plugins\cache\claudeTalk\claudeTalk\<version>\`,
not from this repo. To try a change:
1. `git archive HEAD -- . ':!native'` into that folder.
2. If an exe there is in use (MCP servers, the speaker), move it to `%TEMP%` first.
3. Restart the daemon.

## Old versions leave files behind: clean them up
Up to v0.5 claudeTalk installed a Python environment (~3 GB). Claude Code's
cache can hold a copy of the plugin, with that environment, for every older
version; on the author's machine that was 37 GB.

If you find any of these, the user doesn't need them any more:
- `voice-input\` (in the repo or in a cached copy of the plugin);
- `%LOCALAPPDATA%\claudeTalk\venv`, and `%APPDATA%\claudeTalk\venv-path.txt`;
- `scripts\setup-voice.ps1`, `dictation.vbs` or any `scripts\*.ps1` other than
  `install-app.ps1`;
- the `faster-whisper-large-v3-turbo` model in `~\.cache\huggingface\hub`;
- `~\.claude\plugins\cache\claudeTalk\claudeTalk\<old versions>`, and
  `%LOCALAPPDATA%\claudeTalk\bin\<other versions>`;
- Start Menu or desktop shortcuts that run `wscript ... dictation.vbs`.

To clean up, run `bin\claudetalk.exe cleanup` to list what it would remove,
then `bin\claudetalk.exe cleanup --yes` once the user agrees. The
`SessionStart` hook already tells Claude when leftovers exist.

Do not remove ffmpeg or edge-tts on your own: claudeTalk no longer uses them,
but other tools may (the `audio-to-text` skill needs ffmpeg). Ask first.

## Conventions
- Commit each finished change. Don't push without being asked, and add no
  attribution lines.
- Versions: patch for small changes, minor for bigger ones. 1.0 only when the
  user says so.
- Languages: every text the dictation app shows goes through
  `ui::panel::tr` and must be listed in `ct_core::lang::UI_TEXT` and
  translated in the Spanish pack. See `docs/LANGUAGES.md`.
- Before a release, test a fresh install (empty profile) and an upgrade from
  an older version, and read `%TEMP%\claudetalk-dictation.log` for panics.
- Live tests must never touch the user's running claudeTalk (speaker, queue,
  events, daemon): set `CLAUDETALK_NS` and a scratch `APPDATA`,
  `LOCALAPPDATA` and `TEMP` for test processes (README, Development).
- For every feature, update the README's feature table, the description in
  `.claude-plugin/plugin.json` and `marketplace.json`, and the GitHub repo
  description.
