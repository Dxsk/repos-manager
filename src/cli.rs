use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

use crate::providers::Provider;
use crate::update::REFRESH_ARG;

#[derive(Debug, Parser)]
#[command(
    name = "repos-manager",
    bin_name = "repos-manager",
    version,
    about = "Multi-provider Git repository manager",
    long_about = "Multi-provider Git repository manager.\n\nClone and update every repository you can access on GitHub, GitLab, Forgejo/Gitea, Bitbucket and Radicle into <base-dir>/<host>/<owner>/<repo>.",
    after_help = "Config: ~/.config/repos-manager/config.json\n\nExamples:\n  repos-manager init\n  repos-manager login\n  repos-manager github sync\n  repos-manager sync --all --parallel 8\n  repos-manager status"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// GitHub (uses the gh CLI)
    Github(ProviderCmd),
    /// GitLab (uses the glab CLI)
    Gitlab(ProviderCmd),
    /// Forgejo / Gitea (tea login, API listing)
    #[command(alias = "gitea")]
    Forgejo(ProviderCmd),
    /// Bitbucket (uses the bitbucket CLI or the API)
    Bitbucket(ProviderCmd),
    /// Radicle (uses the rad CLI)
    Radicle(ProviderCmd),
    /// Authenticate (all detected providers if none specified)
    Login { provider: Option<ProviderArg> },
    /// Sync all configured providers
    Sync(SyncAllArgs),
    /// Show dirty/ahead/behind repos across all providers
    Status(StatusArgs),
    /// Create the default config file
    Init,
    /// Check for a new release and self-update
    Update {
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Print the version
    Version,
    /// Print a shell completion script to stdout
    Completions { shell: Shell },
    #[command(name = REFRESH_ARG, hide = true)]
    RefreshUpdateCache,
}

#[derive(Debug, Args)]
#[command(subcommand_required = true, arg_required_else_help = true)]
pub struct ProviderCmd {
    #[command(subcommand)]
    pub action: ProviderAction,
}

#[derive(Debug, Subcommand)]
pub enum ProviderAction {
    /// Authenticate with this provider
    Login,
    /// Sync repositories from this provider
    Sync(SyncArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ProviderArg {
    Github,
    Gitlab,
    #[value(alias = "gitea")]
    Forgejo,
    Bitbucket,
    Radicle,
}

impl From<ProviderArg> for Provider {
    fn from(p: ProviderArg) -> Self {
        match p {
            ProviderArg::Github => Provider::Github,
            ProviderArg::Gitlab => Provider::Gitlab,
            ProviderArg::Forgejo => Provider::Forgejo,
            ProviderArg::Bitbucket => Provider::Bitbucket,
            ProviderArg::Radicle => Provider::Radicle,
        }
    }
}

#[derive(Debug, Clone, Default, Args)]
pub struct CommonFlags {
    /// Base directory (default: ~/Documents)
    #[arg(long, value_name = "PATH")]
    pub base_dir: Option<PathBuf>,
    /// Show debug output
    #[arg(short, long)]
    pub verbose: bool,
    /// Suppress info/success messages (errors still shown)
    #[arg(short, long)]
    pub quiet: bool,
}

#[derive(Debug, Clone, Default, Args)]
pub struct SyncArgs {
    /// Filter repos by pattern (e.g. owner/* or owner/project)
    #[arg(long, value_name = "PATTERN")]
    pub filter: Option<String>,
    /// Use HTTPS instead of SSH
    #[arg(long)]
    pub https: bool,
    /// Remove local repos that no longer exist on the remote
    #[arg(long)]
    pub prune: bool,
    /// Show what would be done without making changes
    #[arg(long)]
    pub dry_run: bool,
    /// Custom host (self-hosted instances), overrides the configured hosts
    #[arg(long)]
    pub host: Option<String>,
    /// Number of parallel sync jobs (default: 4)
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..))]
    pub parallel: Option<u16>,
    #[command(flatten)]
    pub common: CommonFlags,
}

#[derive(Debug, Args)]
pub struct SyncAllArgs {
    /// Sync every provider whose CLI or credentials are available
    #[arg(long, required = true)]
    pub all: bool,
    #[command(flatten)]
    pub sync: SyncArgs,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[command(flatten)]
    pub common: CommonFlags,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("repos-manager").chain(args.iter().copied()))
    }

    #[test]
    fn cli_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn provider_sync_flags() {
        let cli = parse(&[
            "github",
            "sync",
            "--filter",
            "Dxsk/*",
            "--https",
            "--prune",
            "--dry-run",
            "--parallel",
            "8",
            "-q",
        ])
        .unwrap();
        let Some(Command::Github(ProviderCmd {
            action: ProviderAction::Sync(a),
        })) = cli.command
        else {
            panic!("wrong parse");
        };
        assert_eq!(a.filter.as_deref(), Some("Dxsk/*"));
        assert!(a.https && a.prune && a.dry_run && a.common.quiet);
        assert_eq!(a.parallel, Some(8));
    }

    #[test]
    fn equals_form_and_gitea_alias() {
        let cli = parse(&[
            "gitea",
            "sync",
            "--host=git.example.org",
            "--base-dir=/tmp/x",
        ])
        .unwrap();
        let Some(Command::Forgejo(ProviderCmd {
            action: ProviderAction::Sync(a),
        })) = cli.command
        else {
            panic!("wrong parse");
        };
        assert_eq!(a.host.as_deref(), Some("git.example.org"));
        assert_eq!(a.common.base_dir, Some(PathBuf::from("/tmp/x")));
    }

    #[test]
    fn sync_requires_all() {
        assert!(parse(&["sync"]).is_err());
        assert!(parse(&["sync", "--all", "--parallel", "2"]).is_ok());
    }

    #[test]
    fn rejects_unknown_flag_and_zero_parallel() {
        assert!(parse(&["github", "sync", "--nope"]).is_err());
        assert!(parse(&["github", "sync", "--parallel", "0"]).is_err());
    }

    #[test]
    fn login_provider_is_validated() {
        assert!(parse(&["login", "gitea"]).is_ok());
        assert!(parse(&["login", "sourceforge"]).is_err());
    }

    #[test]
    fn provider_arg_maps_to_provider() {
        let all = [
            ProviderArg::Github,
            ProviderArg::Gitlab,
            ProviderArg::Forgejo,
            ProviderArg::Bitbucket,
            ProviderArg::Radicle,
        ];
        let mapped: Vec<Provider> = all.into_iter().map(Into::into).collect();
        assert_eq!(mapped, Provider::ALL);
    }

    #[test]
    fn status_and_update_flags() {
        let cli = parse(&["status", "-v", "--base-dir", "/b"]).unwrap();
        let Some(Command::Status(s)) = cli.command else {
            panic!("wrong parse");
        };
        assert!(s.common.verbose && !s.common.quiet);
        assert!(matches!(
            parse(&["update", "-y"]).unwrap().command,
            Some(Command::Update { yes: true })
        ));
    }
}
