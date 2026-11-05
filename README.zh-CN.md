<p align="center">
  <img src="./public/mascot/mascot-icon.png" alt="Acode" width="96" height="96" />
</p>

<h1 align="center">Acode</h1>

<p align="center">放在桌面上的助手：提问、写代码、处理文档，一个快捷键就能唤出。</p>

<p align="center">
  <a href="./README.md">English</a>
  &nbsp;·&nbsp; 简体中文
  &nbsp;·&nbsp; <a href="https://github.com/Jason-GGB/Acode/releases">下载</a>
  &nbsp;·&nbsp; <a href="./docs/README.zh-CN.md">文档</a>
</p>

<p align="center"><sub>Windows 10 / 11 · macOS · v0.1.0 · MIT</sub></p>

---

## 能做什么

Acode 跑在你自己的电脑上。在任意应用里唤出悬浮窗，或在完整工作台里管理工作区、工具和文件预览。

| 功能         | 你可以做                                                                         |
| ------------ | -------------------------------------------------------------------------------- |
| **随时提问** | 双击 Alt 打开悬浮窗，并附上文件或图片。在 macOS 上这个键是 Option。              |
| **处理项目** | 保留工作区与会话，执行任务，审查变更。                                           |
| **选择模型** | 使用 DeepSeek，或任意 OpenAI 兼容、Anthropic 兼容的服务商。                      |
| **处理文档** | 生成、读取、转换和渲染 Word、Excel、PowerPoint，再以卡片交付。                   |
| **扩展助手** | 添加技能、MCP 和插件。电脑操控（Windows）和 OpenCLI 需要手动开启。               |
| **连接手机** | 通过 [Companion](https://github.com/rururunu/AnyaAndroid) 对话、审批和传输文件。 |
| **本地资料** | 设置名称和头像，查看用量，导入或导出个人资料。                                   |

DeepSeek 模型走内置的 dsh 工具集，同时保留你已启用的插件和 MCP。其他模型使用 Acode 的通用工具。详见 [DeepSeek 链路](./docs/deepseek-harness.zh-CN.md)。

## 安装

从 [Releases](https://github.com/Jason-GGB/Acode/releases) 下载安装包。

| 平台                   | 安装包                        |
| ---------------------- | ----------------------------- |
| macOS（Apple 芯片）    | `Acode_<version>_aarch64.dmg` |
| Windows 10 / 11（x64） | `Acode_<version>_x64.msi`     |

在 macOS 上打开磁盘映像，把 Acode 拖进「应用程序」。当前安装包未做 Apple 公证；如果系统拦截首次打开，到 **系统设置 → 隐私与安全性** 中允许 Acode。

1. 打开 **设置**，添加模型服务商并填写凭据。
2. 在工作台开始对话，或双击 **Alt** / **Option** 打开悬浮窗。

备用快捷键是 <kbd>Ctrl</kbd> + <kbd>Alt</kbd> + <kbd>Space</kbd>。

## 日常使用

| 模式  | 用来做                               |
| ----- | ------------------------------------ |
| Ask   | 只读提问和调研                       |
| Agent | 使用工具完成任务，按你的审批策略执行 |
| Plan  | 先给出计划，批准后才写入             |
| Image | 用单独配置的生图服务生成图片         |

权限与上面的模式相互独立：

| 权限       | 效果                                                                   |
| ---------- | ---------------------------------------------------------------------- |
| 只读       | 阻止修改文件和任意 Shell 命令                                          |
| 工作区写入 | 可以编辑工作区内的文件；每次执行 Shell，或访问工作区外的路径时都会询问 |
| 完全访问   | 跳过这些审批提示，命令安全规则仍然生效                                 |

一次批准只放行这一次操作，也不能越过只读限制。工作台或悬浮窗重新获得焦点时，未完成的问题和审批会恢复。详见 [权限与代码预览](./docs/permissions-and-preview.zh-CN.md)。

在输入框里，`#` 提示技能、插件和 MCP，`@` 提示工作区文件，`/` 提示命令。编辑已发送的消息时同样可用。模型和思考强度在右侧；窗口变窄时收成图标。

双击 Alt 会在悬浮窗取得焦点前取走当前选中的文字，并显示成一个紧凑标签。Chrome 和 Edge 可以安装可选的[选区桥接](./browser-extension/README.zh-CN.md)，用于网页选区和来源地址。

有新版本时，从工作台侧栏或 **设置 → 关于** 打开说明，确认后再安装。

Office 的排版、转换和公式结果需要逐个文件核对。部分搜索和扩展功能需要额外配置。见 [Office 工作流](./docs/office.zh-CN.md) 和 [插件](./docs/plugin-system.zh-CN.md)。

## 数据与隐私

设置、凭据和聊天记录默认留在这台电脑上。发送消息时，文本和附带的上下文会交给你配置的模型服务商。开启搜索、远程 MCP、云同步或手机连接后，相关数据也会发到对应的服务或设备。

自定义背景是本地文件，设置里只保存路径。导出便携资料时会带上能够读取的图片。导出的资料可能包含设置和 API Key，请当作敏感文件保管。个人资料操作里可以恢复出厂设置。

## 开发

| 环境           | 说明                                             |
| -------------- | ------------------------------------------------ |
| Node.js 22.19+ | Office 运行时会打包构建时使用的这个 Node         |
| pnpm 10        |                                                  |
| Rust stable    |                                                  |
| Windows        | Visual Studio C++ 生成工具，以及 WebView2 运行时 |
| macOS          | Xcode 命令行工具                                 |

```bash
pnpm install
pnpm tauri:dev
```

```bash
pnpm check          # 类型、lint 和单元测试
cd src-tauri && cargo test --lib && cd ..
pnpm tauri:build    # 为当前系统打安装包
```

技术栈是 Tauri 2、Vue 3、TypeScript 和 Rust。签名与应用内更新见 [发布与更新](./docs/release.zh-CN.md)。

## 文档

- [文档索引](./docs/README.zh-CN.md)
- [技术架构](./docs/architecture-overview.zh-CN.md)
- [DeepSeek 链路](./docs/deepseek-harness.zh-CN.md)
- [Office 工作流](./docs/office.zh-CN.md)
- [文件交付](./docs/file-delivery.zh-CN.md)
- [浏览器选区桥接](./browser-extension/README.zh-CN.md)
- [插件开发](./docs/plugin-system.zh-CN.md)
- [发布与更新](./docs/release.zh-CN.md)

IDE 上下文插件：[VS Code](https://marketplace.visualstudio.com/items?itemName=Acode.acode-ide-context) · [IntelliJ Platform](https://plugins.jetbrains.com/plugin/33163-acode-ide-context)。

## 许可

[MIT](./LICENSE)。第三方声明见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。

感谢 [Ghost](https://github.com/NORTHTEKDevs/ghost)、[OpenCLI](https://github.com/jackwener/OpenCLI)，以及项目里使用的开源工具和素材。
