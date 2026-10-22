# 发布流程

发布由 `.github/workflows/release.yml` 完成：推送形如 `vX.Y.Z` 的 tag，在 macOS runner 上构建通用（arm64 + x86_64）包，再创建**草稿** GitHub Release，附带 `.dmg` 和 `SHA256SUMS`。草稿不会自动公开，需要维护者检查后手动点 Publish。

`0.x` 版本和带后缀的版本（如 `1.0.0-rc.1`）会被标记为 pre-release。

> 这个流程只经过 `actionlint` 静态检查，引用的 action 的 commit SHA 已与对应 tag 核对。**它还没有在 GitHub 上真实跑过**，第一次打 tag 时请留意运行结果。

## 发版步骤

1. 确认 `main` 上 CI 全绿。
2. 同步修改三处版本号，必须一致，否则发布流程会在第一步失败：
   - `package.json` 的 `version`
   - `src-tauri/Cargo.toml` 的 `version`
   - `src-tauri/tauri.conf.json` 的 `version`
3. 更新 `CHANGELOG.md`。
4. 提交并推送，然后打 tag：

   ```bash
   git tag -a v0.1.0 -m "v0.1.0"
   git push origin v0.1.0
   ```

5. 在 Actions 页面观察 Release 工作流。完成后到 Releases 页面检查草稿：说明文字、`.dmg`、`SHA256SUMS`，确认无误后点 Publish。
6. 重新跑同一个 tag 的工作流会替换草稿里的附件和说明。

## 本地试构建

```bash
npm ci
npm run tauri build                                    # 当前架构
rustup target add x86_64-apple-darwin aarch64-apple-darwin
npm run tauri build -- --target universal-apple-darwin  # 通用包
```

产物在 `src-tauri/target/release/bundle/`（通用包在 `src-tauri/target/universal-apple-darwin/release/bundle/`）。

检查是否签名：

```bash
codesign -dv --verbose=2 "path/to/App.app"
spctl --assess --type execute -vv "path/to/App.app"
```

## 签名与公证（可选）

没有配置签名密钥时，工作流会构建**未签名**版本，并在 Release 说明里提示用户移除隔离标记。要发布签名并公证的版本，需要 Apple Developer Program 账号，并在仓库 Settings > Secrets and variables > Actions 里添加**全部**下列 secret（缺任何一个都会退回到未签名构建，并给出警告）：

| Secret | 内容 |
|---|---|
| `APPLE_CERTIFICATE` | “Developer ID Application” 证书导出的 `.p12`，base64 编码：`base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | 导出 `.p12` 时设置的密码 |
| `APPLE_SIGNING_IDENTITY` | 证书的完整名称，如 `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Apple ID 邮箱 |
| `APPLE_PASSWORD` | 该 Apple ID 的 App 专用密码（appleid.apple.com 生成，不是登录密码） |
| `APPLE_TEAM_ID` | 10 位 Team ID |

签名密钥只暴露给构建任务，该任务只有只读令牌；创建 Release 的任务有写权限，但不运行项目代码，也拿不到签名密钥。

签名构建会在构建后用 `codesign --verify`、`spctl --assess` 和 `stapler validate` 检查。Entitlements 保持为空（`src-tauri/Entitlements.plist` 里解释了原因）；新增条目时必须同时说明为什么需要。

## 版本与更新

应用没有自动更新机制，用户从 Releases 页面重新下载安装。
