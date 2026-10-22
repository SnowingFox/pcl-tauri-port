# Plain Craft Launcher (PCL) Tauri Port (Third-Party)

一个让 Plain Craft Launcher (PCL) 的核心功能能在 macOS 上运行的第三方移植版，使用 Tauri 2 + Rust + React + TypeScript 重新实现。

本项目是第三方基于 [Plain Craft Launcher (PCL)](https://github.com/Meloong-Git/PCL) 独立进行二次创作的产物，与 PCL 官方及其作者无关，也不代表官方立场。

> 当前是 **0.1.0 预发布版**。功能尚未全部验证，见下方「已知限制」。

## 署名

原作者：龙腾猫跃（LTCatt）

- PCL 下载与赞助：<https://meloong.com/pcl>
- 赞助链接：<https://meloong.com/afd/a/LTCat>
- 原版源码：<https://github.com/Meloong-Git/PCL>

macOS 侧的实现思路参考了 [PCL-Community/PCL.Mac](https://github.com/PCL-Community/PCL.Mac)，仅参考逻辑与流程。

如果你喜欢 Minecraft，请支持正版：<https://www.minecraft.net/>。离线账号启动前会弹出购买和赞助提示，这是 PCL 许可证对启动功能的要求。

## 功能

- 账号：离线、微软（设备码登录）、第三方 Yggdrasil / authlib-injector。令牌存放在 macOS 钥匙串。
- 启动：解析版本 JSON、拼装启动参数、解压 natives、自动检测并选择 Java、缺失文件自动补全、崩溃分析。
- 下载：分段并发、断点续传、哈希校验；官方源与 BMCLAPI 镜像；原版与 Fabric、Quilt、Forge、NeoForge 安装。
- 资源：Modrinth 与 CurseForge 搜索与安装（Mod、资源包、光影、数据包），本地 Mod 管理，`.mrpack` 与 CurseForge 整合包安装。
- 实例：多个 `.minecraft` 目录、实例重命名、复制、删除（默认移入废纸篓）、在 Finder 中打开。

## 安装

1. 从 [Releases](https://github.com/SnowingFox/pcl-tauri-port/releases) 下载 `.dmg`，打开后把应用拖进「应用程序」。
2. 校验下载：把 `SHA256SUMS` 与 `.dmg` 放在同一目录，运行 `shasum -a 256 -c SHA256SUMS`。
3. 如果发布的是**未签名版本**，macOS 会在首次启动时拦截。把应用复制到「应用程序」后，执行一次：

   ```bash
   xattr -dr com.apple.quarantine "/Applications/Plain Craft Launcher (PCL) Tauri Port (Third-Party).app"
   ```

要求：macOS 11 或更高。游戏需要自行安装 Java（例如 `brew install openjdk@21`），启动器会自动扫描并按游戏版本选择合适的 Java。

## 配置

| 环境变量 | 用途 |
|---|---|
| `PCL_MS_CLIENT_ID` | 微软登录用的 Azure 应用 Client ID。**没有内置**，需要你自己注册。 |
| `PCL_CURSEFORGE_KEY` | CurseForge API Key，没有它无法搜索或下载 CurseForge 内容。申请：<https://console.curseforge.com/> |
| `PCL_DATA_DIR` | 覆盖应用数据目录，用于测试或便携使用。默认 `~/Library/Application Support/com.snowingfox.pcltauriport` |

也可以在登录窗口里直接填写 Client ID（保存到设置）。

### 注册 Azure 应用（微软登录）

1. Azure Portal > Microsoft Entra ID > App registrations > New registration。
2. Supported account types 选择只含个人 Microsoft 账号的选项。
3. Authentication 里开启 “Allow public client flows”（设备码流程需要）。
4. 复制 Application (client) ID，设为 `PCL_MS_CLIENT_ID`，或在登录窗口里填写。
5. Mojang 可能要求你的应用先通过 Minecraft 服务的审核，见 <https://aka.ms/AppRegInfo>。

## 从源码构建

需要 Node.js 22、Rust stable、Xcode 命令行工具。

```bash
npm ci
npm run tauri dev          # 开发模式
npm run tauri build        # 生成 .app 与 .dmg（当前架构）
```

在浏览器里只看界面（不连后端，使用内置 mock）：`npm run dev`。

检查：

```bash
npm run build && npm test
cd src-tauri && cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked
```

真实联网与真实 JVM 的端到端测试默认忽略，需要本机有 JDK：

```bash
cd src-tauri
PCL_E2E_JAVA=/path/to/bin/java cargo test --locked --lib e2e -- --ignored --nocapture
```

## 已知限制

- **微软登录、CurseForge 在没有你提供的 Client ID 和 API Key 时无法使用**，其真实流程没有在真实服务上验证过。
- 通用（arm64 + x86_64）构建只由 CI 的发布流程产出；本地默认只构建当前架构。
- 未实现：联机、主题商店、皮肤和披风上传、改名、HMCL / MCBBS / PCL 整合包格式、Mod 更新检查的 CurseForge 路径。
- 界面没有逐页对照 PCL 原版，布局与原版不同。
- 没有自动更新。
- 发布版默认**未签名、未公证**，除非仓库配置了 Apple Developer ID，见 [docs/RELEASING.md](docs/RELEASING.md)。

## 安全

威胁模型、已修复的问题和仍然存在的风险见 [docs/SECURITY.md](docs/SECURITY.md)。请通过 GitHub 的私有安全公告反馈漏洞，不要公开提 issue。

## 许可

本仓库继续使用 PCL 的《分发有限许可》与《存储库合理使用指南》，详见 [LICENCE](./LICENCE)。按其中「重度使用」的要求：

- 本项目是第三方基于 PCL 的二次创作，名称以 Plain Craft Launcher (PCL) 开头并带有第三方后缀。
- 关于页首位署名龙腾猫跃并附赞助链接。
- 源代码公开，并继续使用同一份许可文件。
- 离线账号启动前提供正版购买与赞助劝导。
- 界面默认主色为蓝色。
