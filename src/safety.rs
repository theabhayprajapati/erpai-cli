use crate::cli::Global;
use crate::config::Profile;
use crate::error::{CliError, Result};
use crate::output::Context;
use serde_json::Value;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

pub fn resolve_app(g: &Global) -> Result<String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    resolve_app_in(g, &cwd)
}

pub fn resolve_app_in(g: &Global, cwd: &Path) -> Result<String> {
    if let Some(a) = g.app.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(a.to_string());
    }
    if let Ok(s) = std::fs::read_to_string(cwd.join(".erpai").join("app")) {
        let a = s.trim();
        if !a.is_empty() {
            return Ok(a.to_string());
        }
    }
    Err(CliError::validation("app id required").with_hint(
        "pass --app <app-id>, set ERPAI_APP_ID, or write the id to .erpai/app in this directory (find ids with `erpai apps list`)",
    ))
}

pub fn preflight_app(profile: &Profile, app: &str) -> Result<()> {
    if profile.allowed_apps.is_empty() || profile.allowed_apps.iter().any(|a| a == app) {
        return Ok(());
    }
    Err(
        CliError::forbidden(format!("app {app} is outside this credential's allowed apps")).with_hint(
            format!(
                "profile '{}' may access only: {} — run `erpai login` and pick this app, or use another profile",
                profile.name,
                profile.allowed_apps.join(", ")
            ),
        ),
    )
}

pub fn require_filter(filter: &Value, all_rows: bool) -> Result<()> {
    if all_rows {
        return Ok(());
    }
    let has_conditions = filter
        .get("conditions")
        .and_then(Value::as_array)
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    let has_groups = filter
        .get("filterGroups")
        .and_then(Value::as_array)
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    if has_conditions || has_groups {
        return Ok(());
    }
    Err(
        CliError::validation("filter is empty — this would affect every row in the table")
            .with_hint(
                "add conditions to --filter, or pass --all-rows --yes if you really mean every row",
            ),
    )
}

#[derive(Debug, Clone, Copy)]
pub struct Gate {
    pub yes: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Proceed,
    DryRun,
}

impl Gate {
    pub fn from(g: &Global) -> Self {
        Self {
            yes: g.yes,
            dry_run: g.dry_run,
        }
    }

    pub fn confirm(&self, what: &str, ctx: &Context) -> Result<Decision> {
        if self.dry_run {
            return Ok(Decision::DryRun);
        }
        if self.yes {
            return Ok(Decision::Proceed);
        }
        let stdin = std::io::stdin();
        if !stdin.is_terminal() {
            return Err(
                CliError::validation(format!("refusing to {what} without confirmation"))
                    .with_hint("pass --yes to confirm, or --dry-run to preview"),
            );
        }
        let target = format!(
            "{}{}",
            ctx.org.as_deref().unwrap_or("?"),
            ctx.app
                .as_deref()
                .map(|a| format!("/{a}"))
                .unwrap_or_default()
        );
        eprint!(
            "About to {what} in {target} (profile {}). Type 'yes' to continue: ",
            ctx.profile
        );
        std::io::stderr().flush().ok();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        if line.trim() == "yes" {
            Ok(Decision::Proceed)
        } else {
            Err(CliError::validation("cancelled"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Profile;

    fn g(app: Option<&str>) -> Global {
        Global {
            profile: "default".into(),
            app: app.map(str::to_string),
            format: Default::default(),
            yes: false,
            dry_run: false,
        }
    }

    #[test]
    fn app_flag_wins_then_file_then_error() {
        assert_eq!(
            resolve_app_in(&g(Some("a1")), Path::new("/nonexistent")).unwrap(),
            "a1"
        );
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join(".erpai")).unwrap();
        std::fs::write(d.path().join(".erpai/app"), "a2\n").unwrap();
        assert_eq!(resolve_app_in(&g(None), d.path()).unwrap(), "a2");
        let e = resolve_app_in(&g(None), Path::new("/nonexistent")).unwrap_err();
        assert_eq!(e.code.as_str(), "validation_error");
    }

    #[test]
    fn preflight_blocks_apps_outside_the_key() {
        let mut p = Profile::new("default", "https://apps.erp.ai", "k");
        assert!(preflight_app(&p, "any").is_ok());
        p.allowed_apps = vec!["a1".into()];
        assert!(preflight_app(&p, "a1").is_ok());
        assert_eq!(
            preflight_app(&p, "a2").unwrap_err().code.as_str(),
            "forbidden"
        );
    }

    #[test]
    fn empty_filter_is_refused_unless_all_rows() {
        let empty = serde_json::json!({"conditions": [], "logicalOperator": "and"});
        assert_eq!(
            require_filter(&empty, false).unwrap_err().code.as_str(),
            "validation_error"
        );
        assert!(require_filter(&empty, true).is_ok());
        assert!(require_filter(
            &serde_json::json!({"conditions":[{"colId":"c","opr":"eq","value":1}]}),
            false
        )
        .is_ok());
        assert!(require_filter(&serde_json::json!({"filterGroups":[{}]}), false).is_ok());
    }

    #[test]
    fn gate_dry_run_and_yes() {
        let ctx = Context {
            profile: "default".into(),
            org: None,
            app: Some("a1".into()),
        };
        assert!(matches!(
            Gate {
                yes: false,
                dry_run: true
            }
            .confirm("delete 3 records", &ctx)
            .unwrap(),
            Decision::DryRun
        ));
        assert!(matches!(
            Gate {
                yes: true,
                dry_run: false
            }
            .confirm("delete 3 records", &ctx)
            .unwrap(),
            Decision::Proceed
        ));
    }
}
