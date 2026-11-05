# Official plugins

Source of truth for plugins Acode ships and copies into the user plugins
directory on launch (`%APPDATA%\Acode\plugins\` / `Acode Debug\plugins\`).

| Folder | Id | What it is |
| --- | --- | --- |
| [`terminal/`](./terminal/) | `terminal` | In-app ConPTY + xterm (review strip). Agent tool `plugin_terminal__run`. |
| [`computer-use/`](./computer-use/) | `computer-use` | Agent: `launch` / keyboard / UIA / pixels. Screenshot + control tree. Windows playbook. **No UI. Disabled until Enable.** Windows. |
| [`examples/`](./examples/) | — | Reference plugins, not auto-enabled. |

Edit files here, then restart (or reload) Acode. `ensure_bundled_plugins` overwrites the AppData copies from these sources.

The in-app terminal prepends every `<plugin>/bin` to `PATH`.

Pin-window AI badge (PixPin / Snipaste) is not a plugin. Optional local API while Acode is running:

```text
POST http://127.0.0.1:18480/api/ask/image
{"image":"data:image/png;base64,..."}
```
