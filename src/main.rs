mod cli;
mod config;
mod git;
mod http;
mod matcher;
mod output;
mod providers;
mod status;
mod sync;
#[cfg(test)]
mod test_server;
mod update;

use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{CommandFactory, Parser};

use cli::{Cli, Command, CommonFlags, ProviderAction, SyncArgs};
use config::Settings;
use providers::Provider;
use sync::SyncOptions;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {e:#}", output::red("error:"));
            ExitCode::FAILURE
        }
    }
}

fn apply_common(settings: &mut Settings, flags: &CommonFlags) {
    output::set_quiet(flags.quiet);
    output::set_verbose(flags.verbose);
    if let Some(dir) = &flags.base_dir {
        settings.base_dir = dir.clone();
    }
}

fn apply_sync(settings: &mut Settings, args: &SyncArgs) -> SyncOptions {
    apply_common(settings, &args.common);
    if args.https {
        settings.use_https = true;
    }
    if let Some(n) = args.parallel {
        settings.parallel = n.into();
    }
    SyncOptions {
        filter: args.filter.clone(),
        prune: args.prune,
        dry_run: args.dry_run,
    }
}

/// Commands that touch repositories need git, a usable base dir and show the update banner.
fn prepare(settings: &Settings) -> Result<()> {
    if !git::available() {
        bail!("git is required but was not found in PATH");
    }
    settings.validate_base_dir()?;
    update::banner(settings);
    update::refresh_async(settings);
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else {
        Cli::command().print_help()?;
        return Ok(());
    };

    // Commands that need neither config nor git.
    match &command {
        Command::Version => {
            println!("repos-manager {}", update::CURRENT);
            return Ok(());
        }
        Command::Completions { shell } => {
            clap_complete::generate(
                *shell,
                &mut Cli::command(),
                "repos-manager",
                &mut std::io::stdout(),
            );
            return Ok(());
        }
        Command::RefreshUpdateCache => return update::refresh_cache(),
        Command::Update { yes } => return update::self_update(*yes),
        Command::Init => return config::init_config(),
        _ => {}
    }

    let mut settings = Settings::load()?;
    match command {
        Command::Github(p) => provider_cmd(Provider::Github, p.action, settings),
        Command::Gitlab(p) => provider_cmd(Provider::Gitlab, p.action, settings),
        Command::Forgejo(p) => provider_cmd(Provider::Forgejo, p.action, settings),
        Command::Bitbucket(p) => provider_cmd(Provider::Bitbucket, p.action, settings),
        Command::Radicle(p) => provider_cmd(Provider::Radicle, p.action, settings),
        Command::Login { provider } => {
            prepare(&settings)?;
            login(provider.map(Into::into))
        }
        Command::Sync(args) => {
            if args.sync.host.is_some() {
                bail!(
                    "--host targets a single provider, use: repos-manager <provider> sync --host <host>"
                );
            }
            let opts = apply_sync(&mut settings, &args.sync);
            prepare(&settings)?;
            sync_all(&settings, &opts);
            Ok(())
        }
        Command::Status(args) => {
            apply_common(&mut settings, &args.common);
            prepare(&settings)?;
            status::status_all(&settings);
            Ok(())
        }
        Command::Version
        | Command::Completions { .. }
        | Command::RefreshUpdateCache
        | Command::Update { .. }
        | Command::Init => unreachable!("handled above"),
    }
}

fn provider_cmd(provider: Provider, action: ProviderAction, mut settings: Settings) -> Result<()> {
    match action {
        ProviderAction::Login => {
            prepare(&settings)?;
            provider.login()
        }
        ProviderAction::Sync(args) => {
            let opts = apply_sync(&mut settings, &args);
            prepare(&settings)?;
            let hosts = match &args.host {
                Some(h) => vec![h.clone()],
                None => settings.hosts(provider).to_vec(),
            };
            if hosts.is_empty() {
                bail!("no hosts configured for provider {provider}");
            }
            let multi = hosts.len() > 1;
            let mut failed = false;
            for host in &hosts {
                if multi {
                    output::heading(&format!("--- {provider} @ {host} ---"));
                }
                if let Err(e) = sync::sync_host(provider, host, &settings, &opts) {
                    output::error(&format!("{host}: {e:#}"));
                    failed = true;
                }
            }
            if failed {
                bail!("sync finished with errors");
            }
            Ok(())
        }
    }
}

fn login(provider: Option<Provider>) -> Result<()> {
    if let Some(p) = provider {
        return p.login();
    }
    let found: Vec<Provider> = Provider::ALL
        .into_iter()
        .filter(|p| p.available())
        .collect();
    if found.is_empty() {
        bail!("no provider CLI found (gh, glab, tea, bitbucket, rad)");
    }
    for p in found {
        output::heading(&format!("=== Login {p} ==="));
        if let Err(e) = p.login() {
            output::error(&format!("{p}: {e:#}"));
        }
    }
    Ok(())
}

/// One failing provider or host never stops the others.
fn sync_all(settings: &Settings, opts: &SyncOptions) {
    for provider in Provider::ALL {
        if !provider.available() {
            output::debug(&format!(
                "{provider}: {} not found, skipped",
                provider.cli()
            ));
            continue;
        }
        for host in settings.hosts(provider) {
            output::heading(&format!("=== Syncing {provider} @ {host} ==="));
            if let Err(e) = sync::sync_host(provider, host, settings, opts) {
                output::error(&format!("{host}: {e:#}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn sync_flags_override_settings() {
        let mut settings = Settings::for_tests(&PathBuf::from("/from/config"));
        let args = SyncArgs {
            filter: Some("o/*".into()),
            https: true,
            prune: true,
            dry_run: true,
            parallel: Some(7),
            common: CommonFlags {
                base_dir: Some(PathBuf::from("/from/flag")),
                ..Default::default()
            },
            ..Default::default()
        };
        let opts = apply_sync(&mut settings, &args);
        assert_eq!(settings.base_dir, PathBuf::from("/from/flag"));
        assert!(settings.use_https);
        assert_eq!(settings.parallel, 7);
        assert_eq!(opts.filter.as_deref(), Some("o/*"));
        assert!(opts.prune && opts.dry_run);
    }

    #[test]
    fn defaults_keep_settings() {
        let mut settings = Settings::for_tests(&PathBuf::from("/base"));
        let opts = apply_sync(&mut settings, &SyncArgs::default());
        assert_eq!(settings.base_dir, PathBuf::from("/base"));
        assert!(!settings.use_https);
        assert_eq!(settings.parallel, 4);
        assert!(opts.filter.is_none() && !opts.prune && !opts.dry_run);
    }
}
