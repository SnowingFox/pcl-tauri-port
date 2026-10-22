//! Rule engine. Port of PCL `CrashAnalyzer.Analyze` (Crit1 -> Crit2 -> stack -> Crit3).

use super::reasons::Reason;
use super::text::*;

/// Prepared log texts, one per source (any may be missing).
#[derive(Debug, Default, Clone)]
pub struct Logs {
    pub mc: Option<String>,
    pub mc_debug: Option<String>,
    pub hs: Option<String>,
    pub crash: Option<String>,
}

pub struct Analyzer<'a> {
    logs: &'a Logs,
    all: String,
    pub reasons: Vec<(Reason, Vec<String>)>,
}

fn has(reasons: &[(Reason, Vec<String>)], r: Reason) -> bool {
    reasons.iter().any(|(x, _)| *x == r)
}

impl<'a> Analyzer<'a> {
    pub fn new(logs: &'a Logs) -> Self {
        let all = format!(
            "{}{}{}",
            logs.mc.as_deref().or(logs.mc_debug.as_deref()).unwrap_or(""),
            logs.hs.as_deref().unwrap_or(""),
            logs.crash.as_deref().unwrap_or("")
        );
        Self { logs, all, reasons: vec![] }
    }

    pub fn log_all(&self) -> &str {
        &self.all
    }

    fn add(&mut self, r: Reason, extra: Vec<String>) {
        if let Some((_, v)) = self.reasons.iter_mut().find(|(x, _)| *x == r) {
            v.extend(extra);
            *v = dedup(std::mem::take(v));
        } else {
            self.reasons.push((r, extra));
        }
    }
    fn flag(&mut self, r: Reason) {
        self.add(r, vec![]);
    }
    fn add1(&mut self, r: Reason, s: impl Into<String>) {
        let s: String = s.into();
        self.add(r, if s.is_empty() { vec![] } else { vec![s] });
    }

    pub fn run(mut self) -> Vec<(Reason, Vec<String>)> {
        self.crit1();
        if !self.reasons.is_empty() {
            return self.reasons;
        }
        self.crit2();
        if !self.reasons.is_empty() {
            return self.reasons;
        }
        // Stack analysis only makes sense with a mod loader around.
        let a = &self.all;
        if a.contains("orge") || a.contains("abric") || a.contains("uilt") || a.contains("iteloader") {
            let mut keywords: Vec<String> = vec![];
            if let Some(c) = &self.logs.crash {
                keywords.extend(stack_keywords(before_first(c, "System Details")));
            }
            if let Some(m) = &self.logs.mc {
                let mut fatals = search(m, r"(/FATAL\] .+?)[\n]+\[");
                if m.contains("Unreported exception thrown!") {
                    fatals.push(between(m, "Unreported exception thrown!", "at oolloo.jlw.Wrapper"));
                }
                for f in fatals {
                    keywords.extend(stack_keywords(&f));
                }
            }
            if let Some(h) = &self.logs.hs {
                keywords.extend(stack_keywords(&between(h, "T H R E A D", "Registers:")));
            }
            if !keywords.is_empty() {
                let keywords = dedup(keywords);
                match self.mod_names(&keywords) {
                    None => self.add(Reason::StackKeywords, keywords),
                    Some(names) => self.add(Reason::StackModNames, names),
                }
                return self.reasons;
            }
        }
        self.crit3();
        self.reasons
    }

    // ---- Crit 1: precise matches, highest priority --------------------------------
    fn crit1(&mut self) {
        let logs = self.logs;
        if logs.mc.is_none() && logs.hs.is_none() && logs.crash.is_none() {
            self.flag(Reason::NoLogs);
            return;
        }

        if let Some(c) = &logs.crash {
            if c.contains("Unable to make protected final java.lang.Class java.lang.ClassLoader.defineClass") {
                self.flag(Reason::JavaTooNew);
            }
            self.config_failure(c);
        }

        if let Some(m) = &logs.mc {
            let c = |s: &str| m.contains(s);
            if c("Unrecognized option:") {
                self.flag(Reason::JvmArgsInvalid);
            }
            if c("Found multiple arguments for option fml.forgeVersion, but you asked for only one") {
                self.flag(Reason::MultipleForgeInJson);
            }
            if c("The driver does not appear to support OpenGL") {
                self.flag(Reason::GraphicsNoOpenGl);
            }
            if c("java.lang.ClassCastException: java.base/jdk") || c("java.lang.ClassCastException: class jdk.") {
                self.flag(Reason::UsingJdk);
            }
            for s in [
                "java.lang.NoSuchMethodError: 'void net.minecraft.client.renderer.texture.SpriteContents.<init>",
                "java.lang.NoSuchMethodError: 'java.lang.String com.mojang.blaze3d.systems.RenderSystem.getBackendDescription",
                "java.lang.NoSuchMethodError: 'void net.minecraft.client.renderer.block.model.BakedQuad.<init>",
                "java.lang.NoSuchMethodError: 'void net.minecraftforge.client.gui.overlay.ForgeGui.renderSelectedItemName",
                "java.lang.NoSuchMethodError: 'void net.minecraft.server.level.DistanceManager",
                "java.lang.NoSuchMethodError: 'net.minecraft.network.chat.FormattedText net.minecraft.client.gui.Font.ellipsize",
            ] {
                if c(s) {
                    self.flag(Reason::OptiFineForgeIncompatible);
                }
            }
            if c("Open J9 is not supported") || c("OpenJ9 is incompatible") || c(".J9VMInternals.") {
                self.flag(Reason::UsingOpenJ9);
            }
            if c("java.lang.NoSuchFieldException: ucp")
                || c("because module java.base does not export")
                || c("java.lang.ClassNotFoundException: jdk.nashorn.api.scripting.NashornScriptEngineFactory")
                || c("java.lang.ClassNotFoundException: java.lang.invoke.LambdaMetafactory")
            {
                self.flag(Reason::JavaTooNew);
            }
            if c("The directories below appear to be extracted jar files. Fix this before you continue.")
                || c("Extracted mod jars found, loading will NOT continue")
            {
                self.flag(Reason::ModsExtracted);
            }
            if c("java.lang.ClassNotFoundException: org.spongepowered.asm.launch.MixinTweaker") {
                self.flag(Reason::MixinBootstrapMissing);
            }
            if c("Couldn't set pixel format") {
                self.flag(Reason::GraphicsPixelFormat);
            }
            if c("java.lang.OutOfMemoryError") || c("an out of memory error") {
                self.flag(Reason::OutOfMemory);
            }
            if c("java.lang.RuntimeException: Shaders Mod detected. Please remove it, OptiFine has built-in support for shaders.") {
                self.flag(Reason::ShadersModWithOptiFine);
            }
            if c("java.lang.NoSuchMethodError: sun.security.util.ManifestEntryVerifier")
                || c("java.lang.NoSuchMethodError: 'void sun.security.util.ManifestEntryVerifier")
            {
                self.flag(Reason::OldForgeNewJava);
            }
            if c("1282: Invalid operation") {
                self.flag(Reason::OpenGl1282);
            }
            if c("signer information does not match signer information of other classes in the same package") {
                let cls = seek(m, r#"class "([^']+)"'s signer information"#).unwrap_or_default();
                self.add1(Reason::SignatureMismatch, trim_crlf_space(&cls));
            }
            if c("Maybe try a lower resolution resourcepack?") {
                self.flag(Reason::TextureTooLarge);
            }
            if c("java.lang.NoSuchMethodError: net.minecraft.world.server.ChunkManager$ProxyTicketManager.shouldForceTicks(J)Z") && c("OptiFine") {
                self.flag(Reason::OptiFineWorldLoad);
            }
            if c("com.electronwill.nightconfig.core.io.ParsingException: Not enough data available")
                && !has(&self.reasons, Reason::ModConfigBroken)
            {
                self.flag(Reason::NightConfigBug);
            }
            if c("Cannot find launch target fmlclient, unable to launch")
                || (c("Invalid paths argument, contained no existing paths")
                    && (c("libraries\\net\\minecraftforge\\fmlcore") || c("libraries/net/minecraftforge/fmlcore")))
            {
                self.flag(Reason::ForgeIncomplete);
            }
            if c("Invalid module name: '' is not a Java identifier") {
                self.flag(Reason::ModNameSpecialChars);
            }
            if c("has been compiled by a more recent version of the Java Runtime (class file version 55.0), this version of the Java Runtime only recognizes class file versions up to")
                || c("java.lang.RuntimeException: java.lang.NoSuchMethodException: no such method: sun.misc.Unsafe.defineAnonymousClass(Class,byte[],Object[])Class/invokeVirtual")
                || c("java.lang.IllegalArgumentException: The requested compatibility level JAVA_11 could not be set. Level is not supported by the active JRE or ASM version")
            {
                self.flag(Reason::ModNeedsJava11);
            }
            if c("Unsupported class file major version")
                || c("Unsupported major.minor version")
                || c("Level is not supported by the active JRE or ASM version")
                || (c("has been compiled by a more recent version of the Java Runtime") && !has(&self.reasons, Reason::ModNeedsJava11))
            {
                self.flag(Reason::JavaIncompatible);
            }
            if c("Invalid maximum heap size") {
                self.flag(Reason::OutOfMemory);
            }
            if c("Could not reserve enough space") {
                self.flag(Reason::OutOfMemory);
            }
            if c("Caught exception from ") {
                let k = seek(m, r"Caught exception from ([^\n]+?)[\n]").unwrap_or_default();
                let names = self.try_mod_name(k.trim_matches(|ch| ch == '\r' || ch == '\n' || ch == ' '));
                self.add(Reason::ModCertain, names);
            }
            if c("DuplicateModsFoundException") {
                let v = search_ci(m, r"\n\t\w+ : [A-Z]:[^\n]+(?:/|\\)([^/\\\n]+?\.jar)");
                self.add(Reason::ModDuplicate, v);
            }
            if c("Found a duplicate mod") {
                let line = seek(m, r"(Found a duplicate mod[^\n]+)").unwrap_or_default();
                self.add(Reason::ModDuplicate, search_ci(&line, r"([^\\/]+\.jar)"));
            }
            if c("Found duplicate mods") {
                self.add(Reason::ModDuplicate, dedup(search(m, r"Mod ID: '(\w+?)' from mod files:")));
            }
            if c("ModResolutionException: Duplicate") {
                let line = seek(m, r"(ModResolutionException: Duplicate[^\n]+)").unwrap_or_default();
                self.add(Reason::ModDuplicate, search_ci(&line, r"([^\\/]+\.jar)"));
            }
            if c("Incompatible mods found!") {
                let raw = seek(m, r"Incompatible mods found![\s\S]+?: ([\s\S]+?)\tat ").unwrap_or_default();
                let cleaned = before_first(&raw, "更多信息：")
                    .replace("Some of your mods are incompatible with the game or each other!", "");
                self.add1(Reason::ModIncompatible, trim_crlf_space(&cleaned));
            }
            if c("Missing or unsupported mandatory dependencies:") {
                let blocks = search_ci(m, r"Missing or unsupported mandatory dependencies:((?:[\r\n]+\t.*)+)");
                let mut items = vec![];
                for b in blocks {
                    for line in b.lines() {
                        let t = line.trim_matches(|ch| ch == '\r' || ch == '\n' || ch == '\t' || ch == ' ');
                        if !t.is_empty() {
                            items.push(t.to_string());
                        }
                    }
                }
                self.add(Reason::ModMissingDependency, dedup(items));
            }

            // ---- macOS specific (not in PCL) ----
            if c("XstartOnFirstThread")
                || c("GLFW may only be used on the main thread")
                || c("NSInternalInconsistencyException") && c("NSWindow drag regions should only be invalidated on the Main Thread")
            {
                self.flag(Reason::MacMissingStartOnFirstThread);
            }
            if c("incompatible architecture") || c("mach-o file, but is an incompatible architecture") {
                self.flag(Reason::MacArchMismatch);
            }
            if c("Unable to locate a Java Runtime")
                || c("No Java runtime present")
                || c("java: command not found")
                || c("无法找到 Java")
            {
                self.flag(Reason::JavaMissing);
            }
        }

        if let Some(h) = &logs.hs {
            if h.contains("The system is out of physical RAM or swap space") || h.contains("Out of Memory Error") {
                self.flag(Reason::OutOfMemory);
            }
            // Windows-only EXCEPTION_ACCESS_VIOLATION driver rules (Intel/AMD/Nvidia) are kept
            // because imported Windows reports are still analysable.
            if h.contains("EXCEPTION_ACCESS_VIOLATION")
                && (h.contains("# C  [ig") || h.contains("# C  [atio") || h.contains("# C  [nvoglv"))
            {
                self.flag(Reason::GraphicsPixelFormat);
            }
            if h.contains("incompatible architecture") {
                self.flag(Reason::MacArchMismatch);
            }
        }

        if let Some(cr) = &logs.crash {
            if cr.contains("maximum id range exceeded") {
                self.flag(Reason::TooManyMods);
            }
            if cr.contains("java.lang.OutOfMemoryError") {
                self.flag(Reason::OutOfMemory);
            }
            if cr.contains("Pixel format not accelerated") {
                self.flag(Reason::GraphicsPixelFormat);
            }
            if cr.contains("Manually triggered debug crash") {
                self.flag(Reason::PlayerTriggeredCrash);
            }
            if cr.contains("has mods that were not found")
                && check(cr, r"(?i)The Mod File [^\n]+optifine[/\\]OptiFine[^\n]+ has mods that were not found")
            {
                self.flag(Reason::OptiFineForgeIncompatible);
            }
            if cr.contains("-- MOD ") {
                let section = between(cr, "-- MOD ", "Failure message:");
                if contains_ci(&section, ".jar") {
                    let f = seek(&section, r"Mod File: (.+)").unwrap_or_default();
                    self.add1(Reason::ModCertain, trim_crlf_space(&f));
                } else {
                    let msg = seek(cr, r"Failure message: ([\w\W]+?)\tMod").unwrap_or_default();
                    self.add1(Reason::ModLoaderError, trim_crlf_space(&msg.replace('\t', " ")));
                }
            }
            if cr.contains("Multiple entries with same key: ") {
                let k = seek(cr, r"Multiple entries with same key: ([^=]+)").unwrap_or_default();
                let names = self.try_mod_name(&trim_crlf_space(&k));
                self.add(Reason::ModCertain, names);
            }
            if cr.contains("LoaderExceptionModCrash: Caught exception from ") {
                let k = seek(cr, r"LoaderExceptionModCrash: Caught exception from ([^\n]+)").unwrap_or_default();
                let names = self.try_mod_name(&trim_crlf_space(&k));
                self.add(Reason::ModCertain, names);
            }
            self.config_failure(cr);
        }
    }

    fn config_failure(&mut self, crash: &str) {
        if !crash.contains("Failed loading config file ") {
            return;
        }
        let modid = seek(crash, r"Failed loading config file .+ for modid ([^\n]+)").unwrap_or_default();
        let cfg = seek(crash, r"Failed loading config file (.+) of type").unwrap_or_default();
        let name = self
            .try_mod_name(modid.trim_matches(|c| c == '\r' || c == '\n'))
            .into_iter()
            .next()
            .unwrap_or_default();
        let cfg = cfg.trim_matches(|c| c == '\r' || c == '\n').to_string();
        self.add(Reason::ModConfigBroken, vec![name, cfg]);
    }

    // ---- Crit 2 ---------------------------------------------------------------
    fn mixin(&mut self, t: &str) -> bool {
        let is_mixin = t.contains("Mixin prepare failed ")
            || t.contains("Mixin apply failed ")
            || t.contains("MixinApplyError")
            || t.contains("MixinTransformerError")
            || t.contains("mixin.injection.throwables.")
            || t.contains(".json] FAILED during )");
        if !is_mixin {
            return false;
        }
        let name = seek(t, r"from mod ([^./ ]+)\] from").or_else(|| seek(t, r"for mod ([^./ ]+) failed"));
        if let Some(n) = name {
            let names = self.try_mod_name(&trim_crlf_space(&n));
            self.add(Reason::ModMixinFailed, names);
            return true;
        }
        if let Some(j) = search(t, r"(?m)^[^\t\n]+[ \[{(]([^ \[{(]+\.[^ ]+)\.json").into_iter().next() {
            let k = j.replace("mixins", "mixin").replace(".mixin", "").replace("mixin.", "");
            let names = self.try_mod_name(&k);
            self.add(Reason::ModMixinFailed, names);
            return true;
        }
        self.flag(Reason::ModMixinFailed);
        true
    }

    fn crit2(&mut self) {
        let logs = self.logs;
        if let Some(m) = &logs.mc {
            let is_mixin = self.mixin(m);
            if m.contains("An exception was thrown, the game will display an error screen and halt.") {
                let e = seek(m, r"the game will display an error screen and halt\.[\n\r]+[^\n]+?Exception: ([\s\S]+?)\n\tat")
                    .unwrap_or_default();
                self.add1(Reason::ForgeError, e.trim_matches(|c| c == '\r' || c == '\n'));
            }
            for marker in [
                "A potential solution has been determined:",
                "A potential solution has been determined, this may resolve your problem:",
                "确定了一种可能的解决方法，这样做可能会解决你的问题：",
            ] {
                if m.contains(marker) {
                    let pat = format!(r"{}\n((?:\s+ - [^\n]+\n)+)", regex::escape(marker));
                    let block = seek(m, &pat).unwrap_or_default();
                    let lines: Vec<String> =
                        block.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
                    self.add1(Reason::FabricSolution, lines.join("\n"));
                }
            }
            if !is_mixin && m.contains("due to errors, provided by ") {
                let k = seek(m, r"due to errors, provided by '([^']+)").unwrap_or_default();
                let names = self.try_mod_name(&trim_crlf_space(&k));
                self.add(Reason::ModCertain, names);
            }
        }
        if let Some(cr) = &logs.crash {
            self.mixin(cr);
            if cr.contains("Suspected Mod") {
                let raw = between(cr, "Suspected Mod", "Stacktrace");
                if !raw.starts_with("s: None") {
                    let suspects = search(&raw, r"\n\t[^(\t]+\(([^)\n]+)");
                    if !suspects.is_empty() {
                        let names = self.try_mod_names(&suspects);
                        self.add(Reason::ModSuspected, names);
                    }
                }
            }
        }
    }

    // ---- Crit 3: low priority -------------------------------------------------
    fn crit3(&mut self) {
        let logs = self.logs;
        if let Some(m) = &logs.mc {
            if !(m.contains("at net.") || m.contains("INFO]"))
                && logs.hs.is_none()
                && logs.crash.is_none()
                && m.chars().count() < 100
            {
                self.add1(Reason::TinyOutput, m.clone());
            }
            if m.contains("Mod resolution failed") {
                self.flag(Reason::ModLoaderError);
            }
            if m.contains("Failed to create mod instance.") {
                let k = seek(m, r"Failed to create mod instance\. ModID: ([^,]+)")
                    .or_else(|| seek(m, r"Failed to create mod instance\. ModId ([^\n]+) for "))
                    .unwrap_or_default();
                let names = self.try_mod_name(k.trim_matches(|c| c == '\r' || c == '\n'));
                self.add(Reason::ModInitFailed, names);
            }
        }
        if let Some(cr) = &logs.crash {
            if cr.contains("\tBlock location: World: ") {
                let b = seek(cr, r"\tBlock: Block\{([^}]+)").unwrap_or_default();
                let l = seek(cr, r"\tBlock location: World: (\([^)]+\))").unwrap_or_default();
                self.add1(Reason::BlockCrash, format!("{b} {l}"));
            }
            if cr.contains("\tEntity's Exact location: ") {
                let t = seek(cr, r"\tEntity Type: ([^\n]+) \(").unwrap_or_default();
                let l = seek(cr, r"\tEntity's Exact location: ([^\n]+)").unwrap_or_default();
                self.add1(Reason::EntityCrash, format!("{t} ({})", l.trim_end_matches(|c| c == '\r' || c == '\n')));
            }
        }
    }

    // ---- Mod name resolution ---------------------------------------------------
    fn try_mod_name(&self, keyword: &str) -> Vec<String> {
        if keyword.is_empty() {
            return vec![String::new()];
        }
        let raw = vec![keyword.to_string()];
        self.mod_names(&raw).unwrap_or(raw)
    }

    fn try_mod_names(&self, keywords: &[String]) -> Vec<String> {
        if keywords.is_empty() {
            return vec![];
        }
        self.mod_names(keywords).unwrap_or_else(|| keywords.to_vec())
    }

    /// Port of `AnalyzeModName`: map mod-id keywords to jar file names using the
    /// crash report's mod list or the debug log's "valid mod file" lines.
    fn mod_names(&self, keywords: &[String]) -> Option<Vec<String>> {
        let kws: Vec<String> = keywords
            .iter()
            .flat_map(|k| k.split('(').map(|s| s.trim_matches(|c| c == ' ' || c == ')').to_string()).collect::<Vec<_>>())
            .filter(|k| !k.is_empty())
            .collect();
        let mut files: Vec<String> = vec![];

        if let Some(crash) = &self.logs.crash {
            const MARK: &str = "A detailed walkthrough of the error";
            if crash.contains(MARK) {
                let fab = "Fabric Mods";
                let mut start = crash.rfind(MARK).map(|i| i + MARK.len()).unwrap_or(0);
                let mut is_fabric = false;
                if let Some(i) = crash.rfind(fab) {
                    if crash.contains(fab) {
                        is_fabric = true;
                        start = start.max(i + fab.len());
                    }
                }
                let details = &crash[start..];
                let lines: Vec<&str> = details
                    .split('\n')
                    .filter(|l| {
                        (contains_ci(l, ".jar") && l.len() - l.replace(".jar", "").len() == 4)
                            || (is_fabric && l.starts_with("\t\t") && !check(l, r"\t\tfabric[\w-]*: Fabric"))
                    })
                    .collect();
                let mut hints: Vec<String> = vec![];
                for k in &kws {
                    let kn = k.to_lowercase().replace('_', "");
                    for l in &lines {
                        let ln = l.to_lowercase().replace('_', "");
                        if !ln.contains(&kn) {
                            continue;
                        }
                        if ln.contains("minecraft.jar") || ln.contains(" forge-") || ln.contains(" mixin-") {
                            continue;
                        }
                        hints.push(l.trim_matches(|c| c == '\r' || c == '\n').to_string());
                        break;
                    }
                }
                for line in dedup(hints) {
                    let name = if is_fabric {
                        seek(&line, r": ([^\n]+) [^\n]+")
                    } else {
                        rx(r"(?i)\(([^\t]+\.jar)\)|(?:\t\t|\| )([^\t|]+\.jar)")
                            .captures(&line)
                            .and_then(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_string()))
                    };
                    if let Some(n) = name {
                        files.push(n);
                    }
                }
            }
        }

        if let Some(dbg) = &self.logs.mc_debug {
            let lines = search(dbg, r"(?m)valid mod file (.*)");
            let mut hints: Vec<String> = vec![];
            for k in &kws {
                for l in &lines {
                    if l.contains(&format!("{{{k}}}")) {
                        hints.push(l.clone());
                    }
                }
            }
            for l in dedup(hints) {
                if let Some(n) = seek(&l, r"(.*) with") {
                    files.push(n);
                }
            }
        }

        let files = dedup(files);
        if files.is_empty() { None } else { Some(files) }
    }
}

fn search_ci(text: &str, pat: &str) -> Vec<String> {
    search(text, &format!("(?i){pat}"))
}

/// Port of `AnalyzeStackKeyword`: pull likely mod-id words out of a stack trace.
pub fn stack_keywords(stack: &str) -> Vec<String> {
    let text = format!("\n{stack}\n");
    let mut found: Vec<String> = vec![];

    // Package prefixes of stack frames. PCL uses a variable-length look-behind/-ahead;
    // here: take dotted identifiers on each line (before any '{') and drop the class
    // and method segments.
    for line in text.lines() {
        let line = before_first(line, "{");
        for m in rx(r"[a-zA-Z_]\w+(?:\.[\w$]+)+").find_iter(line) {
            if m.start() == 0 {
                continue; // PCL requires at least one character before the match
            }
            let segs: Vec<&str> = m.as_str().split('.').collect();
            if segs.len() >= 4 {
                found.push(segs[..segs.len() - 2].join("."));
            }
        }
    }
    // Mixin frames: xxx.xxx.Xxxx$xxxx$xxx(
    for s in search(&text, r"at [^(]+?\.\w+\$\w+\$([\w\$]+?)\$\w+\(") {
        found.push(s.replace('$', "."));
    }
    let found = dedup(found);

    const IGNORE: &[&str] = &[
        "java", "sun", "javax", "jdk", "oolloo", "org.lwjgl", "com.sun", "net.minecraftforge", "paulscode.sound",
        "com.mojang", "net.minecraft", "cpw.mods", "com.google", "org.apache", "org.spongepowered", "net.fabricmc",
        "com.mumfrey", "com.electronwill.nightconfig", "it.unimi.dsi", "MojangTricksIntelDriversForPerformance_javaw",
    ];
    let possible: Vec<String> = found
        .into_iter()
        .filter(|s| !IGNORE.iter().any(|p| s.starts_with(p)))
        .map(|s| s.trim().to_string())
        .collect();
    let possible = dedup(possible);
    if possible.is_empty() {
        return vec![];
    }

    const STOP: &[&str] = &[
        "com", "org", "net", "asm", "fml", "mod", "jar", "sun", "lib", "map", "gui", "dev", "nio", "api", "dsi", "top",
        "mcp", "core", "init", "mods", "main", "file", "game", "load", "read", "done", "util", "tile", "item", "base",
        "fake", "oshi", "impl", "data", "pool", "task", "forge", "setup", "block", "model", "mixin", "event", "unimi",
        "netty", "world", "lwjgl", "fakes", "fabric", "gitlab", "common", "server", "config", "mixins", "compat",
        "loader", "launch", "script", "entity", "assist", "client", "plugin", "modapi", "mojang", "shader", "events",
        "github", "recipe", "render", "packet", "preinit", "preload", "machine", "reflect", "channel", "general",
        "handler", "content", "systems", "modules", "service", "scripts", "network", "fastutil", "optifine", "internal",
        "platform", "override", "fabricmc", "neoforge", "external", "injection", "listeners", "scheduler", "minecraft",
        "universal", "multipart", "neoforged", "microsoft", "transformer", "transformers", "minecraftforge",
        "blockentity", "spongepowered", "electronwill", "concurrent",
    ];
    let mut words: Vec<String> = vec![];
    for st in &possible {
        for (i, w) in st.split('.').enumerate() {
            if i > 3 {
                break;
            }
            if w.chars().count() <= 2 || w.starts_with("func_") {
                continue;
            }
            if STOP.contains(&w.to_lowercase().as_str()) {
                continue;
            }
            words.push(w.trim().to_string());
        }
    }
    let words = dedup(words);
    if words.len() > 10 { vec![] } else { words }
}
