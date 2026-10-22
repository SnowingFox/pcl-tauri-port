const zhCN = {
  app: { name: "PCL Tauri", tagline: "Minecraft 启动器（macOS 移植版）" },
  nav: { launch: "启动", download: "下载", instances: "实例", settings: "设置", others: "更多" },
  common: {
    ok: "确定", cancel: "取消", close: "关闭", delete: "删除", search: "搜索", loading: "加载中…",
    empty: "暂无内容", retry: "重试", copy: "复制", copied: "已复制", enable: "启用", disable: "禁用",
    install: "安装", back: "返回", save: "保存", error: "出错了",
  },
  launch: {
    account: "账号", noAccount: "尚未登录账号", addAccount: "添加账号", switchAccount: "切换账号",
    instance: "游戏实例", noInstance: "没有可用实例，请先到下载页安装", launchBtn: "启动游戏",
    launching: "启动中…", running: "游戏运行中", kill: "结束游戏", log: "启动日志", clearLog: "清空日志",
    needAccount: "请先添加账号", needInstance: "请先选择实例", lastPlayed: "上次游玩",
    never: "从未游玩", loginTitle: "添加账号",
    stage: { preparing: "准备中", checking: "检查文件", downloading: "补全文件", authenticating: "验证账号", starting: "启动中", running: "运行中" },
  },
  account: {
    offline: "离线账号", microsoft: "微软账号", authlib: "第三方验证", name: "玩家名",
    nameHint: "3-16 位字母、数字或下划线", msHint: "在浏览器打开下方链接并输入代码完成登录",
    msCode: "验证码", msOpen: "打开登录页", msWaiting: "等待登录完成…", msStart: "开始登录",
    server: "验证服务器", username: "用户名", password: "密码", login: "登录", remove: "移除账号",
    invalidName: "玩家名格式不正确", loginFailed: "登录失败",
  },
  download: {
    game: "游戏", resources: "资源", chooseVersion: "选择游戏版本", loader: "加载器", loaderVersion: "加载器版本",
    instanceName: "实例名称", installBtn: "开始安装", release: "正式版", snapshot: "快照版", old_beta: "远古测试版",
    old_alpha: "远古内测版", all: "全部", installing: "安装中", installed: "安装完成", failed: "安装失败",
    vanilla: "原版", mod: "模组", modpack: "整合包", resourcepack: "资源包", shader: "光影",
    searchPlaceholder: "搜索资源…", targetInstance: "安装到实例", downloads: "次下载", noResult: "没有找到相关资源",
    installedOk: "已安装", filterVersion: "游戏版本",
  },
  instance: {
    title: "实例管理", overview: "概览", mods: "模组", saves: "存档", settings: "设置",
    selectHint: "从左侧选择一个实例", version: "游戏版本", loader: "加载器", openFolder: "打开文件夹",
    deleteInstance: "删除实例", confirmDelete: "确定删除该实例吗？此操作不可撤销。", modCount: "模组数量",
    noMods: "没有模组", noSaves: "没有存档", size: "大小", set: { java: "Java 路径", memory: "内存 (MB)", jvm: "JVM 参数", followGlobal: "跟随全局设置" },
    favorite: "收藏",
  },
  settings: {
    title: "设置", launch: "启动", java: "Java", downloadSrc: "下载", personalize: "个性化", about: "关于",
    autoJava: "自动选择 Java", javaPath: "Java 路径", scanJava: "扫描 Java", autoMemory: "自动分配内存",
    memory: "最大内存 (MB)", jvmArgs: "JVM 参数", gameArgs: "游戏参数", winSize: "窗口大小", fullscreen: "全屏",
    onLaunch: "游戏启动后启动器", keep: "保持打开", hide: "隐藏", exit: "退出", source: "下载源",
    official: "官方源", bmclapi: "BMCLAPI 镜像", threads: "下载线程数", verify: "校验文件完整性",
    themeMode: "外观", light: "浅色", dark: "深色", system: "跟随系统", hue: "主题色", reduceMotion: "减少动画",
    saved: "设置已保存",
  },
  theme: { blue: "蓝色", purple: "紫色", green: "绿色", orange: "橙色", pink: "粉色", red: "红色" },
  others: {
    help: "帮助", logs: "游戏日志", crash: "崩溃分析", testCrash: "模拟一次崩溃", logEmpty: "暂无日志，启动游戏后将在此显示",
    filter: "过滤", level: "等级", exportLog: "复制全部日志", crashTitle: "游戏已崩溃", crashReason: "可能原因",
    crashAdvice: "建议：检查最近安装的模组，或更换 Java 版本后重试。", exitCode: "退出码",
  },
  about: {
    title: "关于", author: "原作者", authorName: "龙腾猫跃", sponsor: "赞助原作者", sponsorUrl: "https://meloong.com/afd/a/LTCat",
    desc: "本项目是对 Plain Craft Launcher 的第三方非官方移植，使用 Tauri 2 构建，与原作者无隶属关系。",
    licence: "遵循原项目许可协议，源代码保持开源。", version: "版本",
  },
  help: {
    items: [
      { q: "如何开始游戏？", a: "在「启动」页添加账号，选择实例后点击启动游戏。" },
      { q: "如何安装模组？", a: "先在「下载」页安装带有 Fabric / Forge 等加载器的实例，再到「资源」中搜索并安装模组。" },
      { q: "游戏崩溃怎么办？", a: "在「更多 → 崩溃分析」查看原因，必要时更换 Java 版本或禁用最近添加的模组。" },
      { q: "下载很慢？", a: "在「设置 → 下载」中切换为镜像下载源，并适当增加线程数。" },
    ],
  },
};

export default zhCN;
export type Messages = typeof zhCN;
