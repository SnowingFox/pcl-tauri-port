use super::analyzer::Logs;
use super::reasons::Reason;
use super::*;

fn mc(s: &str) -> Logs {
    Logs { mc: Some(s.into()), ..Default::default() }
}
fn ids(l: &Logs) -> Vec<Reason> {
    analyze_logs(l).into_iter().map(|(r, _)| r).collect()
}
fn details(l: &Logs, r: Reason) -> Vec<String> {
    analyze_logs(l).into_iter().find(|(x, _)| *x == r).map(|(_, d)| d).unwrap_or_default()
}

#[test]
fn no_logs() {
    assert_eq!(ids(&Logs::default()), vec![Reason::NoLogs]);
}

#[test]
fn simple_game_log_rules() {
    let cases: &[(&str, Reason)] = &[
        ("Unrecognized option: --foo", Reason::JvmArgsInvalid),
        ("java.lang.OutOfMemoryError: Java heap space", Reason::OutOfMemory),
        ("Could not reserve enough space for 1048576KB object heap", Reason::OutOfMemory),
        ("Invalid maximum heap size: -Xmx99g", Reason::OutOfMemory),
        ("Unsupported class file major version 65", Reason::JavaIncompatible),
        ("java.lang.ClassCastException: class jdk.internal.loader.ClassLoaders$AppClassLoader", Reason::UsingJdk),
        ("java.lang.NoSuchFieldException: ucp", Reason::JavaTooNew),
        ("Open J9 is not supported", Reason::UsingOpenJ9),
        ("Couldn't set pixel format", Reason::GraphicsPixelFormat),
        ("The driver does not appear to support OpenGL", Reason::GraphicsNoOpenGl),
        ("Cannot find launch target fmlclient, unable to launch", Reason::ForgeIncomplete),
        ("Extracted mod jars found, loading will NOT continue", Reason::ModsExtracted),
        ("java.lang.ClassNotFoundException: org.spongepowered.asm.launch.MixinTweaker", Reason::MixinBootstrapMissing),
        ("1282: Invalid operation", Reason::OpenGl1282),
        ("Maybe try a lower resolution resourcepack?", Reason::TextureTooLarge),
        ("Invalid module name: '' is not a Java identifier", Reason::ModNameSpecialChars),
        ("java.lang.RuntimeException: Shaders Mod detected. Please remove it, OptiFine has built-in support for shaders.", Reason::ShadersModWithOptiFine),
        ("Found multiple arguments for option fml.forgeVersion, but you asked for only one", Reason::MultipleForgeInJson),
        ("java.lang.NoSuchMethodError: sun.security.util.ManifestEntryVerifier.<init>", Reason::OldForgeNewJava),
        ("java.lang.NoSuchMethodError: 'void net.minecraft.client.renderer.texture.SpriteContents.<init>(", Reason::OptiFineForgeIncompatible),
        ("Unable to locate a Java Runtime", Reason::JavaMissing),
        ("dlopen(liblwjgl.dylib): mach-o file, but is an incompatible architecture (have 'x86_64', need 'arm64')", Reason::MacArchMismatch),
        ("GLFW error: GLFW may only be used on the main thread", Reason::MacMissingStartOnFirstThread),
    ];
    for (text, want) in cases {
        let got = ids(&mc(&format!("[12:00:00] [main/INFO]: boot\n{text}\n")));
        assert!(got.contains(want), "{text:?} -> {got:?}, wanted {want:?}");
    }
}

#[test]
fn mod_needs_java11_beats_incompatible() {
    let t = "x has been compiled by a more recent version of the Java Runtime (class file version 55.0), this version of the Java Runtime only recognizes class file versions up to 52.0";
    let got = ids(&mc(t));
    assert!(got.contains(&Reason::ModNeedsJava11));
    assert!(!got.contains(&Reason::JavaIncompatible));
}

#[test]
fn duplicate_mods_extracts_jar_names() {
    let t = "net.minecraftforge.fml.loading.moddiscovery.ModDiscoverer: DuplicateModsFoundException\n\tMod ID: 'jei' from mod files: /m/jei-1.jar, /m/jei-2.jar\nFound duplicate mods:\n\tMod ID: 'jei' from mod files: a.jar, b.jar\n";
    let d = details(&mc(t), Reason::ModDuplicate);
    assert!(d.contains(&"jei".to_string()), "{d:?}");
}

#[test]
fn missing_dependencies_listed() {
    let t = "Missing or unsupported mandatory dependencies:\n\tMod ID: 'fabric', Requested by: 'sodium', Expected range: '>=0.90', Actual version: '[MISSING]'\n\tMod ID: 'x', Requested by: 'y'\n\n";
    let d = details(&mc(t), Reason::ModMissingDependency);
    assert_eq!(d.len(), 2, "{d:?}");
    assert!(d[0].starts_with("Mod ID: 'fabric'"));
}

#[test]
fn mixin_failure_names_mod() {
    let t = "[main/ERROR]: Mixin apply for mod sodium failed sodium.mixins.json:core.Foo from mod sodium] from x";
    let r = analyze_logs(&mc(&format!("{t}\nMixin apply failed : boom")));
    assert_eq!(r[0].0, Reason::ModMixinFailed);
    assert_eq!(r[0].1, vec!["sodium".to_string()]);
    // a log with Mixin errors must not also be reported through the weaker "provided by" rule
    let both = "Mixin apply failed x\nuh oh due to errors, provided by 'optifabric'";
    assert!(!ids(&mc(both)).contains(&Reason::ModCertain));
}

#[test]
fn fabric_solution_block() {
    let t = "A potential solution has been determined:\n\t - Install fabric-api, version 0.90 or later.\n\t - Remove mod x.\n";
    let d = details(&mc(t), Reason::FabricSolution);
    assert_eq!(d.len(), 1);
    assert!(d[0].contains("Install fabric-api") && d[0].contains("Remove mod x."));
}

#[test]
fn crash_report_rules() {
    let crash = "---- Minecraft Crash Report ----\nDescription: Manually triggered debug crash\n";
    let r = ids(&Logs { crash: Some(crash.into()), ..Default::default() });
    assert!(r.contains(&Reason::PlayerTriggeredCrash));

    let crash = "-- MOD examplemod --\nMod File: /mods/example-1.0.jar\nFailure message: boom\n";
    let d = details(&Logs { crash: Some(crash.into()), ..Default::default() }, Reason::ModCertain);
    assert_eq!(d, vec!["/mods/example-1.0.jar".to_string()]);

    let crash = "Failed loading config file foo-common.toml of type COMMON for modid foomod: x";
    let d = details(&Logs { crash: Some(crash.into()), ..Default::default() }, Reason::ModConfigBroken);
    assert_eq!(d[1], "foo-common.toml");
}

#[test]
fn suspected_mods_resolve_to_jar_names() {
    let crash = "Suspected Mods: \n\tSodium (sodium)\nStacktrace:\n\nA detailed walkthrough of the error\n\tMods:\n\t\tsodium-fabric-0.5.jar sodium 0.5\n";
    let r = analyze_logs(&Logs { crash: Some(crash.into()), ..Default::default() });
    let (reason, d) = &r[0];
    assert_eq!(*reason, Reason::ModSuspected);
    assert_eq!(d, &vec!["Sodium (sodium)".to_string()].into_iter().map(|_| d[0].clone()).collect::<Vec<_>>());
}

#[test]
fn stack_analysis_finds_keywords_in_loader_crashes() {
    let crash = "---- Minecraft Crash Report ----\nForge\nDescription: Ticking\njava.lang.NullPointerException\n\tat com.coolmod.cool.Thing.run(Thing.java:10)\n\tat net.minecraft.Foo.bar(Foo.java:1)\nSystem Details";
    let r = analyze_logs(&Logs { crash: Some(crash.into()), ..Default::default() });
    assert!(
        matches!(r.first().map(|x| x.0), Some(Reason::StackKeywords) | Some(Reason::StackModNames)),
        "{r:?}"
    );
}

#[test]
fn block_and_entity_low_priority() {
    let crash = "Forge\n\tBlock: Block{mymod:thing}\n\tBlock location: World: (1,2,3), Chunk: x\n";
    let d = details(&Logs { crash: Some(crash.into()), ..Default::default() }, Reason::BlockCrash);
    assert_eq!(d.len(), 1);
    assert!(d[0].starts_with("mymod:thing"));
}

#[test]
fn tiny_output_reported_when_nothing_else_matches() {
    assert_eq!(ids(&mc("Error: boom")), vec![Reason::TinyOutput]);
    assert!(ids(&mc(&format!("[INFO] fine\n{}", "x".repeat(200)))).is_empty());
}

#[test]
fn hs_err_oom_and_arch() {
    let hs = Logs { hs: Some("# There is insufficient memory\n# Out of Memory Error (os_linux.cpp)".into()), ..Default::default() };
    assert!(ids(&hs).contains(&Reason::OutOfMemory));
}

#[test]
fn crit1_short_circuits_lower_rules() {
    let t = "Unrecognized option: --x\nMixin apply failed";
    assert_eq!(ids(&mc(t)), vec![Reason::JvmArgsInvalid]);
}

#[test]
fn formatted_result_has_chinese_message_and_actions() {
    let r = analyze_to_result(&mc("java.lang.OutOfMemoryError"), false);
    assert_eq!(r.reasons[0].id, "out_of_memory");
    assert!(r.reasons[0].message.contains("内存不足"));
    assert!(!r.reasons[0].actions.is_empty());
    assert!(r.summary.contains("错误报告"));

    let none = analyze_to_result(&mc(&format!("[INFO] ok\n{}", "y".repeat(300))), true);
    assert!(none.reasons.is_empty());
    assert!(none.summary.contains("无法确定"));
}

#[test]
fn every_reason_has_a_message() {
    use Reason::*;
    let all = [
        JvmArgsInvalid, ModsExtracted, MixinBootstrapMissing, OutOfMemory, UsingJdk, GraphicsNoOpenGl, UsingOpenJ9,
        JavaTooNew, JavaIncompatible, ModNameSpecialChars, GraphicsPixelFormat, TinyOutput, PlayerTriggeredCrash,
        OpenGl1282, SignatureMismatch, ModCertain, ModSuspected, ModConfigBroken, ModMixinFailed, ModLoaderError,
        ModInitFailed, StackKeywords, StackModNames, OptiFineWorldLoad, BlockCrash, EntityCrash, TextureTooLarge,
        NoLogs, ModDuplicate, ModIncompatible, OptiFineForgeIncompatible, FabricError, FabricSolution, ForgeError,
        OldForgeNewJava, MultipleForgeInJson, TooManyMods, NightConfigBug, ShadersModWithOptiFine, ForgeIncomplete,
        ModNeedsJava11, ModMissingDependency, JavaMissing, MacMissingStartOnFirstThread, MacArchMismatch,
    ];
    let mut seen = std::collections::HashSet::new();
    for r in all {
        // Empty and single-item detail lists must both be safe.
        for extra in [vec![], vec!["a".to_string()], vec!["a".to_string(), "b".to_string()]] {
            assert!(!reasons::describe(r, &extra, "").message.is_empty(), "{r:?}");
        }
        assert!(seen.insert(r.id()), "duplicate id {}", r.id());
    }
}
