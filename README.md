<p align="center">
  <img src="./public/mascot/mascot-icon.png" alt="Acode" width="96" height="96" />
</p>

<h1 align="center">Acode</h1>

<p align="center">A desktop assistant for questions, code, and documents — one shortcut away.</p>

<p align="center">
  English
  &nbsp;·&nbsp; <a href="./README.zh-CN.md">简体中文</a>
  &nbsp;·&nbsp; <a href="https://github.com/Jason-GGB/Acode/releases">Download</a>
  &nbsp;·&nbsp; <a href="./docs/README.md">Documentation</a>
</p>

<p align="center"><sub>Windows 10 / 11 · macOS · v0.1.0 · MIT</sub></p>

---

## What it does

Acode stays on your machine. Open a floating overlay from any app, or work in a full window with workspaces, tools, and file previews.

| Feature                  | What you can do                                                                                          |
| ------------------------ | -------------------------------------------------------------------------------------------------------- |
| **Ask anywhere**         | Double-tap Alt to open the overlay and attach files or images. On macOS this key is Option.              |
| **Work on a project**    | Keep workspaces and chats, run tasks, and review changes.                                                |
| **Choose a model**       | Use DeepSeek, or any OpenAI-compatible or Anthropic-compatible provider.                                 |
| **Handle documents**     | Generate, read, convert, and render Word, Excel, and PowerPoint files, then deliver them as cards.       |
| **Extend the agent**     | Add skills, MCP servers, and plugins. Computer use (Windows) and OpenCLI stay off until you enable them. |
| **Use your phone**       | Chat, approve actions, and transfer files with [Companion](https://github.com/rururunu/AnyaAndroid).     |
| **Keep a local profile** | Set a name and avatar, review usage, and import or export your profile.                                  |

DeepSeek models run on the integrated dsh tool set and still keep the plugins and MCP servers you enabled. Other models use Acode's general tools. Details: [DeepSeek integration](./docs/deepseek-harness.md).

## Install

Download a build from [Releases](https://github.com/Jason-GGB/Acode/releases).

| Platform              | Installer                     |
| --------------------- | ----------------------------- |
| macOS (Apple Silicon) | `Acode_<version>_aarch64.dmg` |
| Windows 10 / 11 (x64) | `Acode_<version>_x64.msi`     |

On macOS, open the disk image and drag Acode into Applications. The current build is not notarized; if the system blocks the first launch, allow Acode in **System Settings → Privacy & Security**.

1. Open **Settings** and add a model provider with its credentials.
2. Start a chat in the workbench, or double-tap **Alt** / **Option** to open the overlay.

The fallback shortcut is <kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>Space</kbd>.

## Everyday use

| Mode  | Use it for                                           |
| ----- | ---------------------------------------------------- |
| Ask   | Read-only questions and research                     |
| Agent | Tasks that can use tools, under your approval policy |
| Plan  | A plan first; writes wait until you approve          |
| Image | Images from a separately configured image provider   |

Permissions are independent of the mode above:

| Permission      | Effect                                                                                             |
| --------------- | -------------------------------------------------------------------------------------------------- |
| Read-only       | Blocks file changes and arbitrary shell commands                                                   |
| Workspace-write | Edits files inside the workspace, and asks before each shell command or path outside the workspace |
| Full access     | Skips those approval prompts and still applies command safety rules                                |

Approving a prompt allows that operation once. An approval cannot override Read-only. Pending questions and approvals come back when you focus the workbench or the overlay. See [permissions and code preview](./docs/permissions-and-preview.md).

In the composer, `#` suggests skills, plugins, and MCP servers, `@` suggests workspace files, and `/` suggests commands. The same suggestions appear when you edit a sent message. Model and thinking controls sit on the right and collapse to icons when the window is narrow.

Double-tap Alt captures the current selection before the overlay takes focus and shows it as a compact tag. Chrome and Edge can add the optional [Selection Bridge](./browser-extension/README.md) for webpage selections and source URLs.

When an update is available, open it from the workbench sidebar or **Settings → About**, read the notes, then install.

Office layout, conversion, and formula results need a check on each file. Some search and extension features need extra setup. See [Office workflows](./docs/office.md) and [plugins](./docs/plugin-system.md).

## Data and privacy

Settings, credentials, and chat history stay on this computer. A message sends its text and attached context to the model provider you configured. Search, remote MCP, cloud sync, and the phone connection also send the related data to those services or devices when you turn them on.

Custom backgrounds are local files; settings store the path. A portable profile export includes images it can read. Treat an exported profile as sensitive: it can contain settings and API keys. A factory reset is available from the profile actions.

## Develop

| Requirement    | Notes                                                  |
| -------------- | ------------------------------------------------------ |
| Node.js 22.19+ | The Office runtime is bundled from this Node binary    |
| pnpm 10        |                                                        |
| Rust stable    |                                                        |
| Windows        | Visual Studio C++ Build Tools and the WebView2 runtime |
| macOS          | Xcode Command Line Tools                               |

```bash
pnpm install
pnpm tauri:dev
```

```bash
pnpm check          # types, lint, and unit tests
cd src-tauri && cargo test --lib && cd ..
pnpm tauri:build    # installer for the current platform
```

Built with Tauri 2, Vue 3, TypeScript, and Rust. Release signing and in-app updates are documented in [Releases and updates](./docs/release.md).

## Documentation

- [Documentation index](./docs/README.md)
- [Architecture](./docs/architecture-overview.md)
- [DeepSeek integration](./docs/deepseek-harness.md)
- [Office workflows](./docs/office.md)
- [File delivery](./docs/file-delivery.md)
- [Browser Selection Bridge](./browser-extension/README.md)
- [Plugin development](./docs/plugin-system.md)
- [Releases and updates](./docs/release.md)

IDE context plugins: [VS Code](https://marketplace.visualstudio.com/items?itemName=Acode.acode-ide-context) · [IntelliJ Platform](https://plugins.jetbrains.com/plugin/33163-acode-ide-context).

## License

[MIT](./LICENSE). Third-party notices are in [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).

Thanks to [Ghost](https://github.com/NORTHTEKDevs/ghost), [OpenCLI](https://github.com/jackwener/OpenCLI), and the open-source tools and artwork used in Acode.
