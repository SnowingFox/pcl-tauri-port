# Plain Craft Launcher (PCL) Tauri 移植版（第三方）

本项目是第三方基于 [Plain Craft Launcher (PCL)](https://github.com/Meloong-Git/PCL) 独立进行二次创作的产物，使用 Tauri 2 + React + TypeScript 重新实现，目标是让启动器能在 macOS 上运行。

与 PCL 官方及其作者无关，也不代表官方立场。

## 署名

原作者：龙腾猫跃（LTCatt）
- PCL 下载与赞助：https://meloong.com/pcl
- 原版源码：https://github.com/Meloong-Git/PCL

macOS 侧的实现思路参考了 [PCL-Community/PCL.Mac](https://github.com/PCL-Community/PCL.Mac)，仅参考逻辑与流程。

## 许可

本仓库继续使用 PCL 的《分发有限许可》与《存储库合理使用指南》，详见 [LICENCE](./LICENCE)。

## 开发

```bash
npm install
npm run tauri dev
```

需要 Rust stable 与 Node.js。
