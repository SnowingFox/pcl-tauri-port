# 更新日志

格式参考 [Keep a Changelog](https://keepachangelog.com/)，版本号遵循 [SemVer](https://semver.org/)。

## [0.1.0] - 2026-10-23

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

### 下载

- 提高打开文件数上限：从 Finder 或 Dock 启动的应用默认只有 256 个，并发下载会触发 `Too many open files` 并反复退避重试。
- 默认下载源改为「官方优先」（设置结构升级到 v2，旧的「镜像优先」默认值会迁移一次；显式选择过「官方源」的不受影响）。
- 按实际表现选择下载源：同一个文件的多个来源交替重试；刚失败、很慢或不响应的来源在 60 秒内排到最后，对所有文件生效；连接虽在但太慢时，只要有健康的备选来源就换源。
- 每个分段单独重试并从断点继续，不再整文件重下；慢连接会被丢弃重试。
- 任务行显示实时并发（文件数、连接数、线程数、CPU 占用）。

### 修复

- 登录窗口输入玩家名时每输入一个字符就失去焦点。
- 重复添加同名离线账号会产生重复条目；现在返回已有账号并选中它，并在启动时合并旧版本留下的重复项。
- 同样的错误提示不再重复堆叠。

### 已知限制

见 [README](README.md#已知限制)。
