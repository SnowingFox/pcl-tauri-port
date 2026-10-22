//! Crash reasons and their user facing (Chinese) descriptions.
//! Texts are adapted from PCL `CrashAnalyzer.GetAnalyzeResult` for macOS.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    JvmArgsInvalid,
    ModsExtracted,
    MixinBootstrapMissing,
    OutOfMemory,
    UsingJdk,
    GraphicsNoOpenGl,
    UsingOpenJ9,
    JavaTooNew,
    JavaIncompatible,
    ModNameSpecialChars,
    GraphicsPixelFormat,
    TinyOutput,
    PlayerTriggeredCrash,
    OpenGl1282,
    SignatureMismatch,
    ModCertain,
    ModSuspected,
    ModConfigBroken,
    ModMixinFailed,
    ModLoaderError,
    ModInitFailed,
    StackKeywords,
    StackModNames,
    OptiFineWorldLoad,
    BlockCrash,
    EntityCrash,
    TextureTooLarge,
    NoLogs,
    ModDuplicate,
    ModIncompatible,
    OptiFineForgeIncompatible,
    #[allow(dead_code)]
    FabricError,
    FabricSolution,
    ForgeError,
    OldForgeNewJava,
    MultipleForgeInJson,
    TooManyMods,
    NightConfigBug,
    ShadersModWithOptiFine,
    ForgeIncomplete,
    ModNeedsJava11,
    ModMissingDependency,
    // macOS specific additions (not in PCL)
    JavaMissing,
    MacMissingStartOnFirstThread,
    MacArchMismatch,
}

impl Reason {
    pub fn id(&self) -> &'static str {
        match self {
            Reason::JvmArgsInvalid => "jvm_args_invalid",
            Reason::ModsExtracted => "mods_extracted",
            Reason::MixinBootstrapMissing => "mixin_bootstrap_missing",
            Reason::OutOfMemory => "out_of_memory",
            Reason::UsingJdk => "using_jdk",
            Reason::GraphicsNoOpenGl => "graphics_no_opengl",
            Reason::UsingOpenJ9 => "using_openj9",
            Reason::JavaTooNew => "java_too_new",
            Reason::JavaIncompatible => "java_incompatible",
            Reason::ModNameSpecialChars => "mod_name_special_chars",
            Reason::GraphicsPixelFormat => "graphics_pixel_format",
            Reason::TinyOutput => "tiny_output",
            Reason::PlayerTriggeredCrash => "player_triggered_crash",
            Reason::OpenGl1282 => "opengl_1282",
            Reason::SignatureMismatch => "signature_mismatch",
            Reason::ModCertain => "mod_certain",
            Reason::ModSuspected => "mod_suspected",
            Reason::ModConfigBroken => "mod_config_broken",
            Reason::ModMixinFailed => "mod_mixin_failed",
            Reason::ModLoaderError => "mod_loader_error",
            Reason::ModInitFailed => "mod_init_failed",
            Reason::StackKeywords => "stack_keywords",
            Reason::StackModNames => "stack_mod_names",
            Reason::OptiFineWorldLoad => "optifine_world_load",
            Reason::BlockCrash => "block_crash",
            Reason::EntityCrash => "entity_crash",
            Reason::TextureTooLarge => "texture_too_large",
            Reason::NoLogs => "no_logs",
            Reason::ModDuplicate => "mod_duplicate",
            Reason::ModIncompatible => "mod_incompatible",
            Reason::OptiFineForgeIncompatible => "optifine_forge_incompatible",
            Reason::FabricError => "fabric_error",
            Reason::FabricSolution => "fabric_solution",
            Reason::ForgeError => "forge_error",
            Reason::OldForgeNewJava => "old_forge_new_java",
            Reason::MultipleForgeInJson => "multiple_forge_in_json",
            Reason::TooManyMods => "too_many_mods",
            Reason::NightConfigBug => "night_config_bug",
            Reason::ShadersModWithOptiFine => "shaders_mod_with_optifine",
            Reason::ForgeIncomplete => "forge_incomplete",
            Reason::ModNeedsJava11 => "mod_needs_java11",
            Reason::ModMissingDependency => "mod_missing_dependency",
            Reason::JavaMissing => "java_missing",
            Reason::MacMissingStartOnFirstThread => "mac_missing_start_on_first_thread",
            Reason::MacArchMismatch => "mac_arch_mismatch",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SuggestedAction {
    /// Stable id the frontend maps to a handler.
    pub id: &'static str,
    pub label: &'static str,
}

const fn act(id: &'static str, label: &'static str) -> SuggestedAction {
    SuggestedAction { id, label }
}

pub struct Described {
    pub message: String,
    pub actions: Vec<SuggestedAction>,
    /// PCL `\h`: remind the user to share the report file rather than a screenshot.
    pub share_hint: bool,
    /// PCL `\e`: remind the user they can read the report for details.
    pub view_report_hint: bool,
}

fn list(items: &[String]) -> String {
    items.iter().map(|s| format!(" - {s}")).collect::<Vec<_>>().join("\n")
}

fn first(items: &[String]) -> &str {
    items.first().map(String::as_str).unwrap_or("")
}

fn loader_msg(prefix: &str, extra: &[String]) -> String {
    if extra.len() == 1 {
        format!("{prefix}提供了以下错误信息：\n{}\n\n请根据上述信息进行对应处理，如果看不懂英文可以使用翻译软件。", extra[0])
    } else {
        format!("{prefix}可能已经提供了错误信息，请根据错误报告中的日志信息进行对应处理，如果看不懂英文可以使用翻译软件。")
    }
}

pub fn describe(reason: Reason, extra: &[String], log_all: &str) -> Described {
    use Reason::*;
    let single = extra.len() == 1;
    let mut share = false;
    let mut view = false;
    let mut actions: Vec<SuggestedAction> = vec![];
    let message: String = match reason {
        JvmArgsInvalid => {
            actions.push(act("edit_jvm_args", "检查 Java 虚拟机参数"));
            "由于 Java 参数有误，导致游戏无法继续运行。\n请检查高级选项中的 Java 虚拟机参数和游戏参数设置是否有误。".into()
        }
        ModsExtracted => {
            actions.push(act("open_mods_folder", "打开 Mod 文件夹"));
            "由于 Mod 文件被解压了，导致游戏无法继续运行。\n直接把整个 Mod 文件放进 Mod 文件夹中即可，若解压就会导致游戏出错。\n\n请删除 Mod 文件夹中已被解压的 Mod，然后再启动游戏。".into()
        }
        OutOfMemory => {
            share = true;
            actions.push(act("increase_memory", "增加分配给游戏的内存"));
            actions.push(act("remove_heavy_content", "移除配置要求较高的材质、Mod、光影"));
            "Minecraft 内存不足，导致其无法继续运行。\n这很可能是因为电脑内存不足、游戏分配的内存不足，或是配置要求过高。\n\n请在启动设置中增加为游戏分配的内存，并删除配置要求较高的材质、Mod、光影。\n如果依然不奏效，请在开始游戏前尽量关闭其他软件，或者……换台电脑？".into()
        }
        UsingOpenJ9 => {
            actions.push(act("change_java", "更换 Java"));
            "游戏因为使用 OpenJ9 而崩溃了。\n请在版本设置的 Java 选项中改用非 OpenJ9 的 Java，然后再启动游戏。".into()
        }
        UsingJdk => {
            actions.push(act("change_java", "更换 Java"));
            actions.push(act("download_java", "下载 Java 8"));
            "游戏似乎因为使用 JDK，或 Java 版本过高而崩溃了。\n请在版本设置的 Java 选项中改用 Java 8，然后再启动游戏。\n如果你没有安装 Java 8，可以从网络中下载、安装一个。".into()
        }
        JavaTooNew => {
            actions.push(act("change_java", "更换 Java"));
            actions.push(act("download_java", "下载较低版本的 Java"));
            "游戏似乎因为你所使用的 Java 版本过高而崩溃了。\n请在版本设置的 Java 选项中改用较低版本的 Java，然后再启动游戏。\n如果没有，可以从网络中下载、安装一个。".into()
        }
        JavaIncompatible => {
            actions.push(act("change_java", "更换 Java"));
            "游戏不兼容你当前使用的 Java。\n请在版本设置的 Java 选项中改用合适版本的 Java，然后再启动游戏。\n如果你没有安装任何 Mod 并且因此崩溃，请向启动器提交反馈。".into()
        }
        ModNameSpecialChars => {
            actions.push(act("open_mods_folder", "打开 Mod 文件夹"));
            "由于有 Mod 的名称包含特殊字符，导致游戏崩溃。\n请尝试修改 Mod 文件名，让它只包含英文字母、数字、减号（-）、下划线（_）和小数点，然后再启动游戏。".into()
        }
        MixinBootstrapMissing => {
            actions.push(act("open_mods_folder", "打开 Mod 文件夹"));
            "由于缺失 MixinBootstrap，导致游戏崩溃。\n请尝试安装 MixinBootstrap。若安装后依然崩溃，可以尝试在文件名前添加英文感叹号。".into()
        }
        ModMissingDependency => {
            actions.push(act("install_dependency", "安装缺失的前置 Mod"));
            if !extra.is_empty() {
                format!("由于未安装正确的前置 Mod，导致游戏退出。\n缺失的依赖项：\n{}\n\n请根据上述信息进行对应处理，如果看不懂英文可以使用翻译软件。", list(extra))
            } else {
                share = true;
                "由于未安装正确的前置 Mod，导致游戏退出。\n请根据错误报告中的日志信息进行对应处理，如果看不懂英文可以使用翻译软件。".into()
            }
        }
        StackKeywords => {
            share = true;
            if single {
                format!("你的游戏遇到了一些问题，启动器为此找到了一个可疑的关键词：{}。\n\n如果你知道某个关键词对应的 Mod，那么有可能就是它引起的错误，你也可以查看错误报告获取详情。", extra[0])
            } else {
                format!("你的游戏遇到了一些问题，启动器为此找到了以下可疑的关键词：\n - {}\n\n如果你知道某个关键词对应的 Mod，那么有可能就是它引起的错误，你也可以查看错误报告获取详情。", extra.join(", "))
            }
        }
        StackModNames | ModSuspected => {
            share = true;
            view = true;
            actions.push(act("disable_mods", "禁用这些 Mod 后重试"));
            if single {
                format!("启动器怀疑名为 {} 的 Mod 导致了游戏出错，但不能完全确定。\n你可以尝试禁用此 Mod，然后观察游戏是否还会崩溃。", extra[0])
            } else {
                format!("启动器怀疑以下 Mod 导致了游戏出错，但不能完全确定：\n{}\n\n你可以尝试依次禁用上述 Mod，然后观察游戏是否还会崩溃。", list(extra))
            }
        }
        ModCertain => {
            share = true;
            view = true;
            actions.push(act("disable_mods", "禁用这些 Mod 后重试"));
            if single {
                format!("名为 {} 的 Mod 导致了游戏出错。\n你可以尝试禁用此 Mod，然后观察游戏是否还会崩溃。", extra[0])
            } else {
                format!("以下 Mod 导致了游戏出错：\n{}\n\n你可以尝试依次禁用上述 Mod，然后观察游戏是否还会崩溃。", list(extra))
            }
        }
        ModMixinFailed => {
            share = true;
            view = true;
            actions.push(act("disable_mods", "禁用这些 Mod 后重试"));
            if extra.is_empty() {
                "部分 Mod 注入失败，导致游戏出错。\n这一般代表着部分 Mod 与其他 Mod 或当前环境不兼容，或是它存在 Bug。\n你可以尝试逐步禁用 Mod，然后观察游戏是否还会崩溃，以此定位导致崩溃的 Mod。".into()
            } else if single {
                format!("名为 {} 的 Mod 注入失败，导致游戏出错。\n这一般代表着它与其他 Mod 或当前环境不兼容，或是它存在 Bug。\n你可以尝试禁用此 Mod，然后观察游戏是否还会崩溃。", extra[0])
            } else {
                format!("以下 Mod 导致了游戏出错：\n{}\n这一般代表着它们与其他 Mod 或当前环境不兼容，或是它存在 Bug。\n你可以尝试依次禁用上述 Mod，然后观察游戏是否还会崩溃。", list(extra))
            }
        }
        ModConfigBroken => {
            actions.push(act("disable_mods", "禁用该 Mod 后重试"));
            match extra.get(1).filter(|s| !s.is_empty()) {
                None => {
                    share = true;
                    view = true;
                    format!("名为 {} 的 Mod 导致了游戏出错。", first(extra))
                }
                Some(cfg) => format!("名为 {} 的 Mod 导致了游戏出错：\n其配置文件 {} 存在异常，无法读取。", first(extra), cfg),
            }
        }
        ModInitFailed => {
            share = true;
            view = true;
            actions.push(act("disable_mods", "禁用这些 Mod 后重试"));
            if single {
                format!("名为 {} 的 Mod 初始化失败，导致游戏无法继续加载。\n你可以尝试禁用此 Mod，然后观察游戏是否还会崩溃。", extra[0])
            } else {
                format!("以下 Mod 初始化失败，导致游戏出错：\n{}\n\n你可以尝试依次禁用上述 Mod，然后观察游戏是否还会崩溃。", list(extra))
            }
        }
        BlockCrash => {
            share = true;
            if single {
                format!("游戏似乎因为方块 {} 出现了问题。\n\n你可以创建一个新世界，并观察游戏的运行情况：\n - 若正常运行，则是该方块导致出错，你或许需要使用一些方式删除此方块。\n - 若仍然出错，问题就可能来自其他原因……", extra[0])
            } else {
                "游戏似乎因为世界中的某些方块出现了问题。\n\n你可以创建一个新世界，并观察游戏的运行情况：\n - 若正常运行，则是某些方块导致出错，你或许需要删除该世界。\n - 若仍然出错，问题就可能来自其他原因……".into()
            }
        }
        ModDuplicate => {
            actions.push(act("open_mods_folder", "打开 Mod 文件夹"));
            if extra.len() >= 2 {
                format!("你重复安装了多个相同的 Mod：\n{}\n\n每个 Mod 只能出现一次，请删除重复的 Mod，然后再启动游戏。", list(extra))
            } else {
                share = true;
                view = true;
                "你可能重复安装了多个相同的 Mod，导致游戏出错。\n\n每个 Mod 只能出现一次，请删除重复的 Mod，然后再启动游戏。".into()
            }
        }
        EntityCrash => {
            share = true;
            if single {
                if extra[0].contains("minecraft:player") {
                    match extra[0].split_once(' ') {
                        Some((_, loc)) => format!("游戏因为位于 {loc} 的玩家实体导致了崩溃。"),
                        None => "游戏因为玩家实体导致了崩溃。".into(),
                    }
                } else {
                    format!("游戏因为实体 {} 导致了崩溃。", extra[0])
                }
            } else {
                format!("游戏因为下列实体导致了崩溃：{}", extra.join("、"))
            }
        }
        OptiFineForgeIncompatible => {
            actions.push(act("reinstall_loader", "按 OptiFine 要求重新安装 Forge"));
            "由于 OptiFine 与当前版本的 Forge 不兼容，导致了游戏崩溃。\n\n请前往 OptiFine 官网（https://optifine.net/downloads）查看 OptiFine 所兼容的 Forge 版本，并严格按照对应版本重新安装游戏。".into()
        }
        ShadersModWithOptiFine => {
            actions.push(act("open_mods_folder", "打开 Mod 文件夹"));
            "无需同时安装 OptiFine 和 Shaders Mod，OptiFine 已经集成了 Shaders Mod 的功能。\n在删除 Shaders Mod 后，游戏即可正常运行。".into()
        }
        OldForgeNewJava => {
            actions.push(act("change_java", "更换 Java"));
            actions.push(act("reinstall_loader", "更新 Forge"));
            "由于低版本 Forge 与当前 Java 不兼容，导致了游戏崩溃。\n\n请尝试以下解决方案：\n - 更新 Forge 到 36.2.26 或更高版本\n - 换用版本低于 1.8.0.320 的 Java".into()
        }
        MultipleForgeInJson => {
            actions.push(act("reinstall_loader", "重新安装 Forge"));
            "可能由于其他启动器修改了 Forge 版本，当前版本的文件存在异常，导致了游戏崩溃。\n请尝试重新全新安装 Forge，而非使用其他启动器修改 Forge 版本。".into()
        }
        PlayerTriggeredCrash => "* 事实上，你的游戏没有任何问题，这是你自己触发的崩溃。\n* 你难道没有更重要的事要做吗？".into(),
        ModNeedsJava11 => {
            actions.push(act("change_java", "更换 Java"));
            actions.push(act("download_java", "下载 Java 11"));
            "你所安装的部分 Mod 似乎需要使用 Java 11 启动。\n请在启动设置的 Java 选择一项中改用 Java 11，然后再启动游戏。\n如果你没有安装 Java 11，你可以从网络中下载、安装一个。".into()
        }
        TinyOutput => {
            share = true;
            format!("程序返回了以下信息：\n{}\n", first(extra))
        }
        OptiFineWorldLoad => {
            share = true;
            actions.push(act("change_optifine", "更换 OptiFine 版本"));
            "你所使用的 OptiFine 可能导致了你的游戏出现问题。\n\n该问题只在特定 OptiFine 版本中出现，你可以尝试更换 OptiFine 的版本。".into()
        }
        GraphicsPixelFormat | GraphicsNoOpenGl => {
            share = true;
            actions.push(act("update_system", "更新 macOS 与显卡驱动"));
            actions.push(act("change_java", "尝试更换 Java"));
            let _ = log_all;
            "你的显卡或系统图形驱动存在问题，导致游戏无法正常运行。\n\n请尝试将 macOS 更新到最新版本。\n如果还是不行，可以尝试更换 Java 版本，或在终端以外的方式重新启动游戏。".into()
        }
        TextureTooLarge => {
            share = true;
            actions.push(act("remove_resourcepack", "移除高清材质"));
            "你所使用的材质分辨率过高，或显卡配置不足，导致游戏无法继续运行。\n\n如果你正在使用高清材质，请将它移除。\n如果你没有使用材质，那么你可能需要换一台配置更好的电脑……".into()
        }
        NightConfigBug => {
            share = true;
            "由于 Night Config 存在问题，导致了游戏崩溃。\n你可以尝试安装 Night Config Fixes 模组，这或许能解决此问题。".into()
        }
        OpenGl1282 => {
            share = true;
            actions.push(act("remove_resourcepack", "移除光影与资源包"));
            "你所使用的光影或材质导致游戏出现了一些问题……\n\n请尝试删除你所添加的这些额外资源。".into()
        }
        TooManyMods => {
            actions.push(act("remove_mods", "删除部分大型 Mod"));
            "你所安装的 Mod 过多，超出了游戏的 ID 限制，导致了游戏崩溃。\n请尝试安装 JEID 等修复 Mod，或删除部分大型 Mod。".into()
        }
        SignatureMismatch => {
            share = true;
            actions.push(act("redownload_game", "重新下载游戏与 Mod"));
            "部分文件或内容校验失败，导致游戏出现了问题。\n\n请尝试删除游戏（包括 Mod）并重新下载，或尝试在重新下载时使用代理。".into()
        }
        ForgeIncomplete => {
            share = true;
            actions.push(act("reinstall_loader", "重新安装 Forge"));
            "由于安装的 Forge 文件丢失，导致游戏无法正常运行。\n请重新安装一次相同版本的 Forge，然后再启动游戏。\n在打包游戏时删除 libraries 文件夹可能导致此错误。".into()
        }
        FabricError => loader_msg("Fabric ", extra),
        ModIncompatible => {
            actions.push(act("disable_mods", "禁用不兼容的 Mod"));
            if single {
                format!("你所安装的 Mod 不兼容：\n{}\n\n请根据上述信息进行对应处理，如果看不懂英文可以使用翻译软件。", extra[0])
            } else {
                share = true;
                "你所安装的 Mod 不兼容，Mod 加载器可能已经提供了错误信息，请根据错误报告中的日志信息进行对应处理，如果看不懂英文可以使用翻译软件。".into()
            }
        }
        ModLoaderError => {
            if !single {
                share = true;
            }
            if single {
                format!("Mod 加载器提供了以下错误信息：\n{}\n\n请根据上述信息进行对应处理，如果看不懂英文可以使用翻译软件。", extra[0])
            } else {
                "Mod 加载器可能已经提供了错误信息，请根据错误报告中的日志信息进行对应处理，如果看不懂英文可以使用翻译软件。".into()
            }
        }
        FabricSolution => {
            if !single {
                share = true;
            }
            if single {
                format!("Fabric 提供了以下解决方案：\n{}\n\n请根据上述信息进行对应处理，如果看不懂英文可以使用翻译软件。", extra[0])
            } else {
                "Fabric 可能已经提供了解决方案，请根据错误报告中的日志信息进行对应处理，如果看不懂英文可以使用翻译软件。".into()
            }
        }
        ForgeError => {
            if !single {
                share = true;
            }
            loader_msg("Forge ", extra)
        }
        NoLogs => {
            share = true;
            "你的游戏出现了一些问题，但启动器未能找到相关记录文件，因此无法进行分析。".into()
        }
        JavaMissing => {
            actions.push(act("change_java", "选择或下载 Java"));
            actions.push(act("download_java", "下载 Java"));
            "启动游戏时找不到可用的 Java。\n请在启动设置的 Java 选项中选择一个已安装的 Java，或下载并安装一个合适版本的 Java（Apple 芯片请选择 arm64 版本）。".into()
        }
        MacMissingStartOnFirstThread => {
            actions.push(act("edit_jvm_args", "检查 JVM 参数"));
            "游戏在 macOS 上需要使用 -XstartOnFirstThread 参数启动，否则窗口系统会在非主线程中被调用而崩溃。\n\n请检查自定义 JVM 参数是否覆盖了该参数，或重置高级选项中的 JVM 参数后再启动游戏。".into()
        }
        MacArchMismatch => {
            actions.push(act("change_java", "更换为与系统架构匹配的 Java"));
            actions.push(act("reinstall_natives", "重新下载游戏运行库"));
            "游戏所使用的本地库（LWJGL 等）与当前 Java 的处理器架构不匹配（例如在 Apple 芯片上用 x86_64 的运行库，或相反）。\n\n请在版本设置的 Java 选项中选择与你的系统架构一致的 Java（Apple 芯片请使用 arm64 的 Java），\n并补全游戏文件以重新下载对应的运行库。".into()
        }
    };
    Described { message, actions, share_hint: share, view_report_hint: view }
}
