# 贡献指南

感谢你愿意帮忙。这是一个 PCL 的第三方移植版，先读一下 [README](README.md) 里的「许可」一节。

## 许可方面要注意

- 参考 PCL 原版源码时，只参考逻辑与流程，不要整段搬运，也不要复制它的图片、字体或任何密钥、Client ID。
- 项目名称必须以 “Plain Craft Launcher (PCL)” 开头并带第三方后缀；不要使用 “PCL 3”、“PCL Pro”、“PCL 手机版” 之类容易混淆的名称。
- 关于页的署名、离线账号的正版劝导、默认蓝色主色不要移除。

## 开发环境

Node.js 22、Rust stable、Xcode 命令行工具。

```bash
npm ci
npm run tauri dev
```

## 提交前检查

和 CI 一致，全部通过再提交：

```bash
npm run build && npm test
cd src-tauri
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

## 约定

- 依赖版本精确锁定（`=1.2.3`，不用范围），新增依赖请说明理由。
- 新增或修改 Tauri 命令，先读 [docs/SECURITY.md](docs/SECURITY.md) 的 “Adding a command”：来自前端的路径和 URL 必须校验，不要执行前端指定的程序，不要把令牌返回给前端，并为恶意输入补一个回归测试。
- 命令和事件的契约写在 `docs/api/`，改动代码时同步更新。前端的命令名只在 `src/api/` 里出现一次。
- 修 bug 先写能复现它的测试，并确认测试在修复前确实失败。
- 测试里不要访问真实的应用数据目录，用 `PCL_DATA_DIR` 指向临时目录。
- 不要提交密钥、令牌、Client ID 或 API Key。

## 提交信息

简短的祈使句，前缀说明范围，例如 `fix(ui): ...`、`feat(accounts): ...`、`docs: ...`。
