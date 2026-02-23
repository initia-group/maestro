# Feature 19: Workspace Profiles & Agent Templates (v0.3)

## Overview

Implement named workspace profiles (collections of projects and agents that can be switched as a unit) and enhanced agent templates (reusable agent configurations that can be spawned via the command palette). Profiles let users quickly switch between contexts (e.g., "dev" vs. "review" vs. "deploy"), while templates standardize agent configurations.

## Dependencies

- **Feature 02** (Configuration System) — profile definitions in TOML.
- **Feature 06** (Agent Lifecycle) — spawning agents from templates.
- **Feature 14** (Command Palette) — template spawning UI.

## Technical Specification

### Workspace Profiles

A profile is a named set of projects and auto-start agents. Switching profiles kills all current agents and spawns the new profile's agents.

#### Configuration

```toml
# ─── Profiles ────────────────────────────────────────

[[profile]]
name = "dev"
description = "Development workflow"

[[profile.project]]
name = "myapp"
path = "/Users/me/dev/myapp"

[[profile.project.agent]]
name = "backend"
command = "claude"
args = ["--model", "opus"]
auto_start = true

[[profile.project.agent]]
name = "tests"
command = "claude"
args = ["--append-system-prompt", "Focus on tests."]
auto_start = true

[[profile.project]]
name = "webui"
path = "/Users/me/dev/webui"

[[profile.project.agent]]
name = "frontend"
command = "claude"
auto_start = true

[[profile]]
name = "review"
description = "Code review workflow"

[[profile.project]]
name = "myapp"
path = "/Users/me/dev/myapp"

[[profile.project.agent]]
name = "reviewer"
command = "claude"
args = ["--append-system-prompt", "You are a code reviewer. Review the recent changes."]
auto_start = true

[[profile]]
name = "deploy"
description = "Deployment workflow"

[[profile.project]]
name = "infra"
path = "/Users/me/dev/infra"

[[profile.project.agent]]
name = "deploy-staging"
command = "claude"
args = ["--append-system-prompt", "Deploy to staging and verify."]
auto_start = true
```

#### Config Structs

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileConfig {
    /// Profile name (e.g., "dev", "review", "deploy").
    pub name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Projects and agents in this profile.
    pub project: Vec<ProjectConfig>,
}
```

Add to `MaestroConfig`:
```rust
pub struct MaestroConfig {
    // ... existing fields ...
    #[serde(default)]
    pub profile: Vec<ProfileConfig>,
    /// Active profile name (None = use top-level project definitions).
    #[serde(default)]
    pub active_profile: Option<String>,
}
```

### Profile Manager

```rust
pub struct ProfileManager {
    profiles: Vec<ProfileConfig>,
    active_profile: Option<String>,
}

impl ProfileManager {
    pub fn new(profiles: Vec<ProfileConfig>, active: Option<String>) -> Self {
        Self {
            profiles,
            active_profile: active,
        }
    }

    /// Get the currently active profile.
    pub fn active(&self) -> Option<&ProfileConfig> {
        self.active_profile.as_ref().and_then(|name| {
            self.profiles.iter().find(|p| p.name == *name)
        })
    }

    /// List all available profiles.
    pub fn list(&self) -> &[ProfileConfig] {
        &self.profiles
    }

    /// Switch to a different profile.
    /// Returns the new profile's projects and agents.
    pub fn switch(&mut self, name: &str) -> Option<&ProfileConfig> {
        if self.profiles.iter().any(|p| p.name == name) {
            self.active_profile = Some(name.to_string());
            self.active()
        } else {
            None
        }
    }
}
```

### Profile Switching Flow

1. User triggers `profile switch <name>` via command palette.
2. App confirms: "Switch to 'review'? This will kill N running agents. [y/n]"
3. On confirm:
   a. `AgentManager::kill_all()`.
   b. Wait for all agents to exit (with timeout).
   c. Clear the sidebar.
   d. Load the new profile's projects and agents.
   e. Spawn all `auto_start` agents.
   f. Rebuild sidebar.

### Enhanced Agent Templates

Templates are enhanced to support:
- Default project assignment.
- Custom environment variables.
- Working directory override.

```toml
[[template]]
name = "reviewer"
command = "claude"
args = ["--append-system-prompt", "You are a code reviewer."]
description = "Code review specialist"
# New fields:
default_project = "myapp"
env = { CODE_REVIEW = "true" }
```

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct TemplateConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub description: Option<String>,
    /// Default project for this template (can be overridden at spawn time).
    pub default_project: Option<String>,
    /// Environment variables set for agents from this template.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// Working directory override.
    pub cwd: Option<std::path::PathBuf>,
}
```

### Template Spawning via Command Palette

When the user runs `spawn <template> <name> [project]`:

1. Look up the template by name.
2. Use `default_project` if project argument is omitted.
3. Validate that the project exists in the current config/profile.
4. Spawn the agent with the template's command, args, and env.

### Command Palette Extensions

New commands for profiles:

```
profile list                    — List available profiles
profile switch <name>           — Switch to a profile
profile current                 — Show active profile name
```

### CLI Integration

Support profile selection at startup:

```bash
maestro --profile dev           # Start with the "dev" profile
maestro --profile review        # Start with the "review" profile
```

Add to CLI args:
```rust
#[derive(Parser)]
struct Cli {
    // ... existing ...
    /// Activate a named workspace profile
    #[arg(short, long)]
    profile: Option<String>,
}
```

## Implementation Steps

1. **Add `ProfileConfig` to config structs**
   - New `[[profile]]` section in TOML.
   - Validation: unique profile names, valid project paths.

2. **Implement `ProfileManager`**
   - Profile listing, switching, active profile tracking.

3. **Implement profile switching in `App`**
   - Kill all agents, load new profile, re-spawn.
   - Confirmation dialog.

4. **Enhance `TemplateConfig`**
   - Add `default_project`, `env`, `cwd` fields.

5. **Update command palette**
   - Add `profile` commands.
   - Template spawning uses enhanced fields.

6. **Add `--profile` CLI flag**

7. **Update sidebar**
   - Show active profile name in the header: "PROJECTS (dev)".

## Error Handling

| Scenario | Handling |
|---|---|
| Unknown profile name | Error message: "Profile 'X' not found. Available: dev, review, deploy". |
| Profile switch with unsaved work | Confirmation prompt warns about running agents. |
| Profile has invalid project path | Log warning for that project, continue with others. |
| No profiles defined | Profile commands are hidden from the command palette. |
| Template not found | Error: "Template 'X' not found". |

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_profile_switch() {
    let profiles = vec![
        ProfileConfig { name: "dev".into(), description: None, project: vec![] },
        ProfileConfig { name: "review".into(), description: None, project: vec![] },
    ];
    let mut manager = ProfileManager::new(profiles, Some("dev".into()));

    assert_eq!(manager.active().unwrap().name, "dev");

    manager.switch("review");
    assert_eq!(manager.active().unwrap().name, "review");
}

#[test]
fn test_profile_switch_nonexistent() {
    let profiles = vec![
        ProfileConfig { name: "dev".into(), description: None, project: vec![] },
    ];
    let mut manager = ProfileManager::new(profiles, None);
    assert!(manager.switch("nonexistent").is_none());
}

#[test]
fn test_profile_list() {
    let profiles = vec![
        ProfileConfig { name: "a".into(), description: None, project: vec![] },
        ProfileConfig { name: "b".into(), description: None, project: vec![] },
    ];
    let manager = ProfileManager::new(profiles, None);
    assert_eq!(manager.list().len(), 2);
}
```

### Integration Tests

- Define config with 2 profiles.
- Start with profile "dev".
- Switch to "review" via command palette.
- Verify old agents are killed and new agents spawned.

## Acceptance Criteria

- [ ] Profiles can be defined in `config.toml` with `[[profile]]`.
- [ ] Each profile has its own set of projects and agents.
- [ ] `profile switch <name>` kills current agents and spawns the new profile's agents.
- [ ] Profile switching includes a confirmation dialog if agents are running.
- [ ] `--profile <name>` CLI flag activates a profile at startup.
- [ ] Templates support `default_project`, `env`, and `cwd` fields.
- [ ] `spawn <template> <name> [project]` works from the command palette.
- [ ] Active profile name is shown in the sidebar header.
- [ ] Invalid profile/template names produce clear error messages.
- [ ] All unit tests pass.
