use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const GRAPHIFY_SKILL_CONTENT: &str = r"# Skill Directive: Graphify AST Semantic Graph First

When searching for code definitions, structs, functions, or call chains across the repository,
you MUST ALWAYS query `graphify-mcp` tools BEFORE falling back to raw file system grepping.

1. **Symbol & Definition Lookup**:
   - Use `graphify_query` or `graphify_trace` first (16ms AST parser for Rust, Py, Go, JS, C, C++, PHP).
2. **Topology & Architecture**:
   - Use `graphify_summary` (returns ultra-compact `.toon` format, saving 60%+ tokens).
";

struct InstallTargets {
    opencode: bool,
    cline: bool,
    cursor: bool,
}

pub fn install_skill(global: bool, target_dir: Option<PathBuf>) -> Result<()> {
    if !global && target_dir.is_none() {
        // Interactive installation flow!
        run_interactive_install()
    } else {
        // Direct non-interactive flow (automated scripts or flags)
        let targets = InstallTargets {
            opencode: true,
            cline: true,
            cursor: true,
        };
        execute_install(
            global,
            target_dir.unwrap_or_else(|| PathBuf::from(".")),
            &targets,
        )
    }
}

fn run_interactive_install() -> Result<()> {
    println!("\x1B[1;36mGraphify Skill Directives Installer 🚀\x1B[0m");
    println!("====================================\n");

    println!("Where do you want to install the Graphify Skill directive?");
    println!(
        "  \x1B[32m[1]\x1B[0m Current Project Level (.opencode/skills/graphify/SKILL.md, .clinerules, .cursorrules)"
    );
    println!(
        "  \x1B[32m[2]\x1B[0m User Global Level (~/.config/opencode/skills/graphify/SKILL.md, ~/.cursorrules)"
    );
    println!("  \x1B[32m[3]\x1B[0m Both (Project & Global)");
    print!("\nEnter your choice [1-3, default: 1]: ");
    io::stdout().flush()?;

    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    let choice = choice.trim();

    let (install_project, install_global) = match choice {
        "2" => (false, true),
        "3" => (true, true),
        _ => (true, false), // Default to 1 (project level)
    };

    println!("\nSelect target AI Assistants / IDEs to configure:");
    println!("  \x1B[32m[1]\x1B[0m OpenCode / Agent Skills (.opencode/skills/)");
    println!("  \x1B[32m[2]\x1B[0m Cline / Roo Code (.clinerules)");
    println!("  \x1B[32m[3]\x1B[0m Cursor (.cursorrules)");
    println!("  \x1B[32m[4]\x1B[0m All of the above (Recommended)");
    print!("\nEnter your choice [1-4, default: 4]: ");
    io::stdout().flush()?;

    let mut assistant_choice = String::new();
    io::stdin().read_line(&mut assistant_choice)?;
    let assistant_choice = assistant_choice.trim();

    let (opencode, cline, cursor) = match assistant_choice {
        "1" => (true, false, false),
        "2" => (false, true, false),
        "3" => (false, false, true),
        _ => (true, true, true), // Default to All
    };

    let targets = InstallTargets {
        opencode,
        cline,
        cursor,
    };

    if install_project {
        println!("\nInstalling to Project Level...");
        execute_install(false, PathBuf::from("."), &targets)?;
    }

    if install_global {
        println!("\nInstalling to Global Level...");
        execute_install(true, PathBuf::from("."), &targets)?;
    }

    println!(
        "\n\x1B[1;32m✔ Skill installed successfully! AI Agents will now prioritize Graphify AST tools over generic grep.\x1B[0m"
    );
    Ok(())
}

fn execute_install(global: bool, base_path: PathBuf, targets: &InstallTargets) -> Result<()> {
    let resolved_base = if global {
        let home = std::env::var("HOME").context("Failed to resolve HOME environment variable")?;
        PathBuf::from(home)
    } else {
        base_path
    };

    if targets.opencode {
        let opencode_dir = if global {
            resolved_base
                .join(".config")
                .join("opencode")
                .join("skills")
                .join("graphify")
        } else {
            resolved_base
                .join(".opencode")
                .join("skills")
                .join("graphify")
        };
        fs::create_dir_all(&opencode_dir)
            .with_context(|| format!("Failed to create directory: {}", opencode_dir.display()))?;
        let skill_file = opencode_dir.join("SKILL.md");
        if skill_file.exists() {
            println!(
                "  - Skipped OpenCode skill (already exists): {}",
                skill_file.display()
            );
        } else {
            fs::write(&skill_file, GRAPHIFY_SKILL_CONTENT)
                .with_context(|| format!("Failed to write: {}", skill_file.display()))?;
            println!("  - Installed OpenCode skill: {}", skill_file.display());
        }
    }

    if targets.cline {
        let rules_file = resolved_base.join(".clinerules");
        append_or_create(&rules_file, GRAPHIFY_SKILL_CONTENT)?;
        println!("  - Installed Cline rules: {}", rules_file.display());
    }

    if targets.cursor {
        let rules_file = resolved_base.join(".cursorrules");
        append_or_create(&rules_file, GRAPHIFY_SKILL_CONTENT)?;
        println!("  - Installed Cursor rules: {}", rules_file.display());
    }

    Ok(())
}

fn append_or_create(path: &Path, content: &str) -> Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    if !existing.contains("Graphify AST Semantic Graph First") {
        let new_content = if existing.is_empty() {
            content.to_string()
        } else {
            format!("{}\n\n{}", existing.trim_end(), content)
        };
        fs::write(path, new_content)
            .with_context(|| format!("Failed to write rules to: {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_temp_dir() -> tempfile::TempDir {
        let Ok(dir) = tempfile::tempdir() else {
            panic!("failed to create temp dir");
        };
        dir
    }

    #[test]
    fn test_install_creates_skill_when_not_exists() {
        let dir = create_temp_dir();
        let targets = InstallTargets {
            opencode: true,
            cline: false,
            cursor: false,
        };

        execute_install(false, dir.path().to_path_buf(), &targets).expect("install should succeed");

        let skill_file = dir.path().join(".opencode/skills/graphify/SKILL.md");
        assert!(skill_file.exists(), "SKILL.md should be created");
        let content = fs::read_to_string(&skill_file).expect("should read file");
        assert!(content.contains("Graphify AST Semantic Graph First"));
    }

    #[test]
    fn test_install_skips_existing_skill() {
        let dir = create_temp_dir();
        let targets = InstallTargets {
            opencode: true,
            cline: false,
            cursor: false,
        };

        // Create existing SKILL.md with old content
        let skill_dir = dir.path().join(".opencode/skills/graphify");
        fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
        let skill_file = skill_dir.join("SKILL.md");
        let original_content =
            "# Old Graphify Skill\n\nThis is old content that should not be overwritten.";
        fs::write(&skill_file, original_content).expect("failed to write original content");

        execute_install(false, dir.path().to_path_buf(), &targets).expect("install should succeed");

        let content = fs::read_to_string(&skill_file).expect("should read file");
        assert_eq!(content, original_content, "SKILL.md should not be modified");
    }

    #[test]
    fn test_install_skips_existing_skill_different_content() {
        let dir = create_temp_dir();
        let targets = InstallTargets {
            opencode: true,
            cline: false,
            cursor: false,
        };

        // Create existing SKILL.md with completely different content
        let skill_dir = dir.path().join(".opencode/skills/graphify");
        fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
        let skill_file = skill_dir.join("SKILL.md");
        let original_content = "Completely unrelated skill content here.";
        fs::write(&skill_file, original_content).expect("failed to write original content");

        execute_install(false, dir.path().to_path_buf(), &targets).expect("install should succeed");

        let content = fs::read_to_string(&skill_file).expect("should read file");
        assert_eq!(content, original_content, "SKILL.md should not be modified");
    }

    #[test]
    fn test_install_reinstall_after_deletion() {
        let dir = create_temp_dir();
        let targets = InstallTargets {
            opencode: true,
            cline: false,
            cursor: false,
        };

        // First install
        execute_install(false, dir.path().to_path_buf(), &targets)
            .expect("first install should succeed");

        let skill_file = dir.path().join(".opencode/skills/graphify/SKILL.md");
        let first_content = fs::read_to_string(&skill_file).expect("should read file");

        // Delete and reinstall
        fs::remove_file(&skill_file).expect("failed to delete skill file");
        execute_install(false, dir.path().to_path_buf(), &targets)
            .expect("reinstall should succeed");

        let second_content =
            fs::read_to_string(&skill_file).expect("should read file after reinstall");
        assert_eq!(
            first_content, second_content,
            "SKILL.md should be recreated with same content"
        );
    }
}
