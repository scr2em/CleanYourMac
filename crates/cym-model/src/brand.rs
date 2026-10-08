//! The ecosystem brand a finding belongs to, as a Simple Icons slug the app draws in the
//! brand's colour. Findings about plain files carry no brand; the app shows their file icon.
use crate::model::Finding;

/// Ecosystem names, as modules report them in the "Ecosystem" detail, and their brands.
/// Matched case-insensitively against the whole name, then against its first word.
const ECOSYSTEMS: &[(&str, &str)] = &[
    (".net", "dotnet"),
    ("android", "android"),
    ("angular", "angular"),
    ("bun", "bun"),
    ("capacitor", "capacitor"),
    ("carthage", "swift"),
    ("cmake", "cmake"),
    ("cocoapods", "cocoapods"),
    ("dart", "dart"),
    ("deno", "deno"),
    ("docker", "docker"),
    ("docusaurus", "docusaurus"),
    ("elixir", "elixir"),
    ("elm", "elm"),
    ("expo", "expo"),
    ("flutter", "flutter"),
    ("flutter / dart", "flutter"),
    ("go", "go"),
    ("golang", "go"),
    ("gradle", "gradle"),
    ("haskell", "haskell"),
    ("homebrew", "homebrew"),
    ("ivy", "openjdk"),
    ("java", "openjdk"),
    ("javascript", "javascript"),
    ("jetbrains", "jetbrains"),
    ("kotlin", "kotlin"),
    ("maven", "apachemaven"),
    ("next.js", "nextdotjs"),
    ("node", "nodedotjs"),
    ("node.js", "nodedotjs"),
    ("nodejs", "nodedotjs"),
    ("npm", "npm"),
    ("nuxt", "nuxt"),
    ("nx", "nx"),
    ("php", "php"),
    ("php (composer)", "composer"),
    ("pnpm", "pnpm"),
    ("podman", "podman"),
    ("python", "python"),
    ("react native", "react"),
    ("ruby", "ruby"),
    ("ruby (bundler)", "rubygems"),
    ("rust", "rust"),
    ("sbt", "scala"),
    ("scala", "scala"),
    ("storybook", "storybook"),
    ("svelte", "svelte"),
    ("sveltekit", "svelte"),
    ("swift", "swift"),
    ("swiftpm", "swift"),
    ("terraform", "terraform"),
    ("turborepo", "turborepo"),
    ("unity", "unity"),
    ("unreal engine", "unrealengine"),
    ("xcode", "xcode"),
    ("yarn", "yarn"),
    ("zig", "zig"),
];

/// Brands for modules whose findings all belong to one tool.
const MODULES: &[(&str, &str)] = &[
    ("xcode", "xcode"),
    ("simulators", "xcode"),
    ("worktrees", "git"),
];

/// AI tools, as the AI Tools module names them in the "Tool" detail, and their brands.
const TOOLS: &[(&str, &str)] = &[
    ("claude code", "claude"),
    ("claude desktop", "claude"),
    ("cline cli", "cline"),
    ("cursor", "cursor"),
    ("gemini cli", "googlegemini"),
    ("github copilot cli", "githubcopilot"),
    ("hugging face", "huggingface"),
    ("lm studio", "lmstudio"),
    ("ollama", "ollama"),
    ("qwen code", "qwen"),
    ("windsurf", "windsurf"),
    ("zed", "zedindustries"),
];

/// Every slug this module can return, so the app's icon set can be checked against it.
pub fn slugs() -> Vec<&'static str> {
    let mut all: Vec<_> = ECOSYSTEMS
        .iter()
        .chain(MODULES)
        .chain(TOOLS)
        .map(|(_, slug)| *slug)
        .chain(["huggingface", "pytorch"])
        .collect();
    all.sort_unstable();
    all.dedup();
    all
}

/// The brand for an ecosystem name such as "Rust (Cargo)" or "Flutter / Dart".
pub fn for_ecosystem(name: &str) -> Option<&'static str> {
    let name = name.trim().to_lowercase();
    let lookup = |key: &str| ECOSYSTEMS.iter().find(|(k, _)| *k == key).map(|(_, s)| *s);
    lookup(&name).or_else(|| {
        let first = name.split([' ', '(', '/']).next().unwrap_or_default();
        lookup(first)
    })
}

/// The brand a finding should show, from its module, ecosystem and package manager.
pub fn of(f: &Finding) -> Option<&'static str> {
    if let Some((_, slug)) = MODULES.iter().find(|(m, _)| *m == f.module_id) {
        return Some(slug);
    }
    if f.module_id == "ai" {
        let tool = f.value("Tool")?.to_lowercase();
        return TOOLS.iter().find(|(t, _)| *t == tool).map(|(_, s)| *s);
    }
    // A known package manager (pnpm, Yarn, Bun) is more specific than its ecosystem.
    if let Some(slug) = f.value("Package manager").and_then(for_ecosystem) {
        return Some(slug);
    }
    let ecosystem = f.value("Ecosystem")?;
    if ecosystem.eq_ignore_ascii_case("machine learning") {
        let title = f.title.to_lowercase();
        return if title.contains("hugging") {
            Some("huggingface")
        } else if title.contains("torch") {
            Some("pytorch")
        } else {
            Some("python")
        };
    }
    for_ecosystem(ecosystem)
}
