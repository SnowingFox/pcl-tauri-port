# 更新日志

格式参考 [Keep a Changelog](https://keepachangelog.com/)，版本号遵循 [SemVer](https://semver.org/)。

## [0.1.0] - 未发布

首个预发布版：把 PCL 的核心启动链路移植到 Tauri 2。

### 新增

- 账号：离线、微软设备码登录、第三方 Yggdrasil / authlib-injector；令牌存放在 macOS 钥匙串。
- 启动：版本 JSON 解析（`inheritsFrom`、规则、新旧参数格式）、classpath 与 natives、macOS 的 `-XstartOnFirstThread`、Java 自动发现与选择、缺失文件自动补全、启动日志与退出事件、崩溃分析（移植了大部分 PCL 规则）。
- 下载：分段并发、断点续传、重试、哈希校验、暂停与取消；官方源与 BMCLAPI 镜像；原版、Fabric、Quilt、Forge、NeoForge 安装。
- 资源：Modrinth 与 CurseForge 搜索与安装；本地 Mod 元数据解析与启用 / 禁用；`.mrpack`、CurseForge、MultiMC 整合包安装。
- 实例与设置：多个 Minecraft 目录、实例管理、全局与实例级设置。
- 界面：React 界面，覆盖启动、下载、实例、设置、更多页；zh-CN；浅色、深色、跟随系统，默认蓝色主色。
- 离线账号启动前的正版购买与赞助提示。
- CI（格式、clippy、测试、前端构建）与打 tag 触发的 macOS 发布流程。

### 安全

- 前端传入的路径、URL、可执行文件全部经过校验（路径穿越、SSRF、zip-slip、任意程序执行、危险 JVM 参数），详见 [docs/SECURITY.md](docs/SECURITY.md)。
- 严格的 CSP、最小权限的 capability、不向前端返回访问令牌。

### 修复

- 登录窗口输入玩家名时每输入一个字符就失去焦点。
- 重复添加同名离线账号会产生重复条目；现在返回已有账号并选中它。

### 已知限制

见 [README](README.md#已知限制)。
