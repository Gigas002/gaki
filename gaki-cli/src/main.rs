use anyhow::Result;
#[cfg(feature = "authentication")]
use anyhow::bail;
use clap::Parser;

#[macro_use]
mod output;
#[cfg(feature = "authentication")]
mod daemon;
#[cfg(feature = "authentication")]
mod proc_admin;

mod cli;
use crate::cli::*;

mod commands;
use crate::commands::*;

/// Print the top-level long help. Used for a bare `gaki-cli` invocation so the
/// user sees every command.
fn print_long_help() {
    use clap::CommandFactory;
    cli_print!("{}", Cli::command().render_long_help());
}

fn main() {
    // Before parsing: a game alias in the arguments is looked up in this dir.
    if let Some(dir) = config_dir_arg(std::env::args().skip(1)) {
        libgaki::core::config::set_config_dir(dir);
    }
    let cli = parse_cli();
    // Global options that must hold before anything reads the config or the socket.
    if let Some(dir) = &cli.config_dir {
        libgaki::core::config::set_config_dir(dir.clone());
    }
    #[cfg(feature = "authentication")]
    if let Some(socket) = &cli.socket {
        daemon::set_socket(socket.clone());
    }

    // Translate any configured network proxy into the standard proxy environment
    // variables *before* the async runtime or worker threads start, so every reqwest
    // client in the process (including those in vendored crates) routes through it.
    // Doing this while the process is still single-threaded keeps the env mutation
    // sound. A spawned `daemon` subprocess re-runs this through its own `main`.
    #[cfg(feature = "authentication")]
    libgaki::core::net::install_proxy_env(&libgaki::core::config::load_proxy_config_blocking());

    // The CLI's top-level async future is large (every command arm) and the Steam
    // connect/auth path is deeply nested; in debug builds this can overflow the OS
    // main thread's default stack before any command runs. Run
    // the Tokio runtime on a thread with a generous stack to avoid that.
    let worker = std::thread::Builder::new()
        .name("gaki-main".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(16 * 1024 * 1024)
                .build()
                .expect("failed to build the Tokio runtime");
            runtime.block_on(async_main(cli));
        })
        .expect("failed to spawn the main worker thread");
    worker.join().expect("the main worker thread panicked");
}

/// Whether this invocation must run in **this** process rather than being forwarded
/// to the session daemon.
///
/// Interactive `login` (default credential prompts, `--qr`, or `--code`/`--pin`) needs
/// the real terminal: it reads the username and **masks the password**. Forwarding it
/// would run `rpassword` inside the daemon — a process with no access to the user's tty
/// — so the password could not be hidden and would be echoed in clear text. The daemon
/// reloads the shared session when `account.toml` changes on the next forwarded
/// command, so logging in locally still updates it. `login --health`/`--reconnect`
/// (daemon-oriented) and `login --json` (non-tty, GUI-driven NDJSON) are not interactive
/// and keep forwarding.
///
/// `kill` and `daemon stop|list` manage local OS processes directly and must likewise
/// never forward to (or auto-spawn) the daemon they may be about to terminate.
#[cfg(feature = "authentication")]
fn must_run_locally(cli: &Cli) -> bool {
    let interactive_login = !cli.json
        && matches!(
            cli.command,
            Command::Login(LoginArgs {
                health: false,
                reconnect: false,
                ..
            })
        );

    // `login --web-token` only writes the session file — no CM connection — so
    // it gains nothing from the daemon's shared session.
    let web_token_login = matches!(
        cli.command,
        Command::Login(LoginArgs {
            web_token: Some(_),
            ..
        })
    );

    // `duplicates` only reads appmanifests off disk — no Steam session, nothing the
    // daemon holds. Forwarding it would gain nothing and cost accuracy: a long-lived
    // daemon answers from the code it was started with, so a freshly fixed scan would
    // still report the old result until the daemon happened to be restarted.
    // Same for `profiles`, which only reads config files.
    #[cfg(feature = "install")]
    let duplicates = matches!(cli.command, Command::Duplicates);
    #[cfg(not(feature = "install"))]
    let duplicates = false;
    let local_disk_scan = duplicates || matches!(cli.command, Command::Profiles { .. });

    // With `reauth_sensitive`, an interactive sensitive read signs in again
    // first, which needs the terminal for the same reason as `login`.
    let interactive_reauth =
        !cli.json && is_sensitive(&cli.command) && libgaki::ops::auth::reauth_sensitive();

    interactive_login
        || web_token_login
        || local_disk_scan
        || interactive_reauth
        || matches!(
            cli.command,
            // `capabilities` describes this binary.
            Command::Kill
                | Command::Capabilities { .. }
                | Command::Daemon {
                    command: Some(_),
                    ..
                }
        )
}

/// The commands that read sensitive account data (`reauth_sensitive`).
#[cfg(feature = "authentication")]
fn is_sensitive(command: &Command) -> bool {
    #[cfg(feature = "inventory")]
    if matches!(command, Command::Inventory { .. } | Command::Wallet) {
        return true;
    }
    #[cfg(feature = "market")]
    if matches!(
        command,
        Command::Market {
            command: MarketCommand::Listings,
        }
    ) {
        return true;
    }
    let _ = command;
    false
}

/// Parse the command line, exiting on errors, `--help` and `--version`.
fn parse_cli() -> Cli {
    match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            use clap::error::ErrorKind;
            // Bare `gaki-cli` (no subcommand): print the full help instead of
            // clap's terse "requires a subcommand" error.
            if matches!(
                e.kind(),
                ErrorKind::MissingSubcommand | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) {
                print_long_help();
                std::process::exit(0);
            }
            // Everything else (genuine parse errors, --help, --version) keeps clap's
            // default rendering and exit codes.
            e.exit();
        }
    }
}

async fn async_main(cli: Cli) {
    // Send tracing/diagnostics to stderr so stdout stays clean for --json output.
    let log = cli.log.as_deref();
    #[cfg(feature = "authentication")]
    if matches!(cli.command, Command::Daemon { command: None }) {
        libgaki::infra::logging::init_daemon_logging(cli.verbose, log);

        // Bare `gaki-cli daemon`: become the shared-session server. Never forwards.
        if let Err(e) = daemon::run_server().await {
            cli_eprintln!("daemon error: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    libgaki::infra::logging::init_cli_logging(cli.verbose, log);

    // Thin client: forward this command to a running daemon (auto-spawning one if
    // needed) so it runs against the single shared Steam session. If no daemon is
    // available, fall through and run locally. `--no-daemon` opts out.
    #[cfg(feature = "authentication")]
    if !must_run_locally(&cli) && !cli.no_daemon {
        use daemon::client::Forwarded;
        match daemon::client::try_forward(cli.no_spawn).await {
            // Exit the process directly rather than returning: the stdin-forwarding
            // task reads `tokio::io::stdin()` on a blocking thread that never
            // finishes for an interactive (no-EOF) stdin, which would otherwise stall
            // the runtime's shutdown and hang exit.
            Ok(Forwarded::Done(code)) => std::process::exit(code),
            // Parent gone; nothing would read a re-run.
            Ok(Forwarded::OutputClosed) => std::process::exit(0),
            Ok(Forwarded::Unavailable) => {}
            Err(e) => tracing::warn!("daemon forwarding failed ({e:#}); running locally"),
        }
    }

    let code = run_and_report(cli).await;
    std::process::exit(code);
}

/// Whether this process serves the config directory a forwarded command asked
/// for (`None`: whichever it serves).
#[cfg(feature = "authentication")]
fn serves_config_dir(requested: Option<&std::path::Path>) -> bool {
    let Some(requested) = requested else {
        return true;
    };
    let canonical =
        |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    libgaki::core::config::config_dir()
        .is_ok_and(|served| canonical(&served) == canonical(requested))
}

/// Parse `argv` and run the resulting command, reporting any error the same way the
/// CLI does. Returns the process exit code. This is the daemon's entry point for a
/// forwarded command (see [`daemon`]); `argv[0]` is the program name.
#[cfg(feature = "authentication")]
pub(crate) async fn run_argv(argv: Vec<String>) -> i32 {
    match Cli::try_parse_from(&argv) {
        Ok(cli) if !serves_config_dir(cli.config_dir.as_deref()) => {
            let served = libgaki::core::config::config_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_default();
            cli_eprintln!(
                "Error: this session daemon serves the config directory {served}; pass \
                 --socket to use a separate daemon for another one, or --no-daemon"
            );
            2
        }
        Ok(cli) => run_and_report(cli).await,
        Err(e) => {
            use clap::error::ErrorKind;
            let rendered = e.render().to_string();
            match e.kind() {
                // --help / --version are "errors" that print to stdout, exit 0.
                ErrorKind::DisplayHelp
                | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                | ErrorKind::DisplayVersion => {
                    cli_print!("{rendered}");
                    0
                }
                _ => {
                    cli_eprint!("{rendered}");
                    2
                }
            }
        }
    }
}

/// Run a parsed command and print any error (JSON or plain), returning the exit code.
async fn run_and_report(cli: Cli) -> i32 {
    let json = cli.json;
    match run(cli).await {
        Ok(()) => 0,
        Err(err) => {
            let classified = libgaki::core::error::classify(&err);
            let reported = err
                .chain()
                .any(|c| c.is::<libgaki::core::error::Reported>());
            if reported {
                // Its own result line said it.
            } else if json {
                // Single line so it stays valid in the NDJSON streams (e.g. `install`).
                let mut value = serde_json::json!({
                    "error": format!("{err:#}"),
                    "type": classified.kind,
                });
                if let Some(hint) = classified.kind.hint() {
                    value["hint"] = hint.into();
                }
                if let Some(retry_after) = classified.retry_after {
                    value["retry_after_seconds"] = retry_after.as_secs().into();
                }
                print_json_line(&value);
            } else {
                cli_eprintln!("Error: {err:#}");
            }
            classified.kind.exit_code()
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    libgaki::core::config::ensure_config_dirs().await?;

    let json = cli.json;
    match cli.command {
        #[cfg(feature = "authentication")]
        Command::Login(args) => cmd_login(args, json).await,
        #[cfg(feature = "authentication")]
        Command::Logout => cmd_logout(json).await,
        #[cfg(feature = "library")]
        Command::List {
            installed,
            search,
            collection,
            online,
            check_updates,
        } => cmd_list(installed, search, collection, online, check_updates, json).await,
        #[cfg(feature = "library")]
        Command::Account => cmd_account(json).await,
        #[cfg(feature = "collections")]
        Command::Collections { command } => match command {
            CollectionsCommand::List => cmd_collections_list(json).await,
            CollectionsCommand::Show { name } => cmd_collections_show(name, json).await,
            CollectionsCommand::Create { name } => cmd_collections_create(name, json).await,
            CollectionsCommand::Delete { name } => cmd_collections_delete(name, json).await,
            CollectionsCommand::Rename { name, new_name } => {
                cmd_collections_rename(name, new_name, json).await
            }
            CollectionsCommand::Add { name, app_ids } => {
                cmd_collections_add(name, app_ids, json).await
            }
            CollectionsCommand::Remove { name, app_ids } => {
                cmd_collections_remove(name, app_ids, json).await
            }
            CollectionsCommand::Pull => cmd_collections_pull(json).await,
            CollectionsCommand::Push { yes } => cmd_collections_push(yes, json).await,
            CollectionsCommand::Sync { yes } => cmd_collections_sync(yes, json).await,
        },
        #[cfg(feature = "social")]
        Command::Friends { command } => match command {
            None | Some(FriendsCommand::List) => cmd_friends(json).await,
            Some(FriendsCommand::Search { query }) => cmd_friends_search(query, json).await,
            Some(FriendsCommand::Add { query }) => cmd_friends_add(query, json).await,
            Some(FriendsCommand::Remove { steamid }) => cmd_friends_remove(steamid, json).await,
        },
        #[cfg(feature = "social")]
        Command::Chat { command } => match command {
            ChatCommand::Send { steamid, message } => {
                cmd_chat_send(steamid, message.join(" "), json).await
            }
            ChatCommand::History { steamid, count } => cmd_chat_history(steamid, count, json).await,
            ChatCommand::Open { steamid } => cmd_chat_open(steamid, json).await,
        },
        #[cfg(feature = "inventory")]
        Command::Inventory { app_id, context } => cmd_inventory(app_id, context, json).await,
        #[cfg(feature = "inventory")]
        Command::Wallet => cmd_wallet(json).await,
        #[cfg(feature = "market")]
        Command::Market { command } => match command {
            MarketCommand::Price {
                app_id,
                name,
                currency,
            } => cmd_market_price(app_id, name, currency, json).await,
            MarketCommand::Search {
                query,
                app_id,
                count,
            } => cmd_market_search(query, app_id, count, json).await,
            MarketCommand::Listings => cmd_market_listings(json).await,
        },
        #[cfg(feature = "install")]
        Command::Install(args) => match args.action {
            Some(InstallAction::List) => cmd_install_list(json).await,
            Some(InstallAction::Stop { app_id }) => cmd_install_stop(app_id, json).await,
            Some(InstallAction::Pause { app_id }) => cmd_install_pause(app_id, json).await,
            Some(InstallAction::Resume { app_id, all }) => {
                cmd_install_resume(app_id, all, json).await
            }
            Some(InstallAction::Forget {
                app_id,
                delete_files,
            }) => cmd_install_forget(app_id, delete_files, json).await,
            None => {
                let Some(app_id) = args.app_id else {
                    bail!("an app id is required, e.g. `gaki-cli install 945360`");
                };
                cmd_install(
                    app_id,
                    args.platform,
                    args.restart_steam,
                    args.dry_run,
                    args.library.map(crate::output::user_dir),
                    json,
                )
                .await
            }
        },
        #[cfg(feature = "library")]
        Command::Libraries => cmd_libraries(json).await,
        #[cfg(feature = "library")]
        Command::Library { command } => match command {
            LibraryCommand::Export { path, all } => {
                let path = path.map(|p| crate::output::user_path(&p));
                cmd_library_export(path, all, json).await
            }
            #[cfg(feature = "install")]
            LibraryCommand::Import {
                path,
                library,
                dry_run,
            } => {
                let path = crate::output::user_path(&path);
                let library = library.map(crate::output::user_dir);
                cmd_library_import(path, library, dry_run, json).await
            }
        },
        #[cfg(feature = "install")]
        Command::Uninstall {
            app_id,
            delete_prefix,
        } => cmd_uninstall(app_id, delete_prefix, json).await,
        #[cfg(feature = "install")]
        Command::Move {
            app_id,
            library,
            restart_steam,
        } => {
            cmd_move(
                app_id,
                crate::output::user_path(&library),
                restart_steam,
                json,
            )
            .await
        }
        #[cfg(feature = "install")]
        Command::Relink {
            app_id,
            library,
            restart_steam,
        } => {
            cmd_relink(
                app_id,
                crate::output::user_path(&library),
                restart_steam,
                json,
            )
            .await
        }
        #[cfg(feature = "install")]
        Command::Import {
            app_id,
            library,
            platform,
            restart_steam,
        } => {
            let library = crate::output::user_path(&library);
            cmd_import(app_id, library, platform, restart_steam, json).await
        }
        #[cfg(feature = "install")]
        Command::Available { app_id } => cmd_available(app_id, json).await,
        #[cfg(feature = "library")]
        Command::Drm { app_id } => cmd_drm(app_id, json).await,
        #[cfg(feature = "install")]
        Command::Verify { app_id } => cmd_verify(app_id, json).await,
        #[cfg(feature = "install")]
        Command::Duplicates => cmd_duplicates(json).await,
        #[cfg(feature = "install")]
        Command::Update { app_id, force } => match app_id {
            Some(id) => cmd_update(id, force, json).await,
            None => cmd_check_updates(json).await,
        },
        #[cfg(feature = "versioning")]
        Command::Manifests { app_id, depot } => cmd_manifests(app_id, depot, json).await,
        #[cfg(feature = "versioning")]
        Command::Downgrade(args) => cmd_downgrade(args, json).await,
        #[cfg(feature = "versioning")]
        Command::Pin { app_id } => cmd_pin(app_id, json).await,
        #[cfg(feature = "versioning")]
        Command::Unpin { app_id } => cmd_unpin(app_id, json).await,
        #[cfg(feature = "library")]
        Command::Play {
            app_id,
            proton,
            steam,
            platform,
            no_update,
        } => cmd_play(app_id, proton, steam, platform, no_update, json).await,
        #[cfg(feature = "library")]
        Command::LaunchTargets { app_id, steam } => cmd_launch_targets(app_id, steam, json).await,
        #[cfg(feature = "library")]
        Command::Running => cmd_running(json),
        #[cfg(feature = "library")]
        Command::Stop { app_id, force } => cmd_stop(app_id, force, json).await,
        #[cfg(feature = "library")]
        Command::Enable {
            app_id,
            restart_steam,
        } => cmd_set_dlc(app_id, true, restart_steam, json).await,
        #[cfg(feature = "library")]
        Command::Disable {
            app_id,
            restart_steam,
        } => cmd_set_dlc(app_id, false, restart_steam, json).await,
        #[cfg(feature = "versioning")]
        Command::Branches { app_id } => cmd_branches(app_id, json).await,
        #[cfg(feature = "versioning")]
        Command::SetBranch { app_id, branch } => cmd_set_branch(app_id, branch, json).await,
        #[cfg(feature = "library")]
        Command::Info {
            app_ids,
            extended,
            no_cache,
            country,
        } => cmd_info(app_ids, extended, no_cache, country, json).await,
        #[cfg(feature = "store")]
        Command::Price {
            app_id,
            compare,
            country,
        } => cmd_price(app_id, compare, country, json).await,
        #[cfg(feature = "store")]
        Command::Search {
            term,
            count,
            country,
        } => cmd_search(term, count, country, json).await,
        #[cfg(feature = "store")]
        Command::Deals {
            scope,
            min_discount,
            count,
            start,
            country,
        } => cmd_deals(scope, min_discount, count, start, country, json).await,
        #[cfg(feature = "store")]
        Command::Similar {
            app_id,
            count,
            country,
        } => cmd_similar(app_id, count, country, json).await,
        #[cfg(feature = "store")]
        Command::Players { app_id } => cmd_players(app_id, json).await,
        #[cfg(feature = "social")]
        Command::User { user } => cmd_user(user, json).await,
        #[cfg(feature = "store")]
        Command::Wishlist {
            user,
            count,
            offset,
            country,
            command,
        } => match command {
            Some(WishlistCommand::Add { app_id }) => cmd_wishlist_add(app_id, json).await,
            Some(WishlistCommand::Remove { app_id }) => cmd_wishlist_remove(app_id, json).await,
            None => cmd_wishlist(user, count, offset, country, json).await,
        },
        #[cfg(feature = "store")]
        Command::Events { country } => cmd_events(country, json).await,
        #[cfg(feature = "store")]
        Command::News {
            app_id,
            count,
            max_length,
        } => cmd_news(app_id, count, max_length, json).await,
        #[cfg(feature = "store")]
        Command::Reviews(args) => cmd_reviews(args, json).await,
        #[cfg(feature = "library")]
        Command::Dlc { app_id } => cmd_dlc(app_id, json).await,
        #[cfg(feature = "library")]
        Command::Achievements { app_id } => cmd_achievements(app_id, json).await,
        #[cfg(feature = "depot")]
        Command::Depots {
            command: Some(command),
            ..
        } => cmd_depots_command(command, json).await,
        #[cfg(feature = "library")]
        Command::Depots { app_id, .. } => {
            // clap requires the app id when no subcommand is given.
            let app_id = app_id.ok_or_else(|| anyhow::anyhow!("missing app id"))?;
            cmd_depots(app_id, json).await
        }
        #[cfg(feature = "library")]
        Command::LaunchOptions { app_id } => cmd_launch_options(app_id, json).await,
        #[cfg(feature = "library")]
        Command::Image {
            app_id,
            output,
            force,
            list,
            probe,
        } => {
            if list {
                cmd_image_list(app_id, probe, json).await
            } else {
                let output = output.map(|p| crate::output::user_path(&p));
                cmd_image(app_id, output, force, json).await
            }
        }
        #[cfg(feature = "library")]
        Command::Tags { dump } => cmd_tags(dump, json).await,
        #[cfg(feature = "library")]
        Command::Appinfo { app_id } => cmd_appinfo(app_id).await,
        Command::Config { command } => match command {
            ConfigCommand::Show => cmd_config_show(json).await,
            ConfigCommand::Check => cmd_config_check(json).await,
            ConfigCommand::Get { key } => cmd_config_get(&key, json).await,
            ConfigCommand::Set { key, value } => cmd_config_set(&key, Some(&value), json).await,
            ConfigCommand::Unset { key } => cmd_config_set(&key, None, json).await,
            ConfigCommand::Protons => cmd_config_protons(json).await,
            ConfigCommand::Presence { mode } => cmd_config_presence(mode, json).await,
            ConfigCommand::Experimental { enabled } => cmd_config_experimental(enabled, json).await,
            ConfigCommand::InstallResume { mode } => cmd_config_install_resume(mode, json).await,
            ConfigCommand::InfoCacheTtl { seconds, clear } => {
                cmd_config_info_cache_ttl(change(clear, seconds), json).await
            }
            ConfigCommand::Proxy {
                url,
                no_proxy,
                clear,
            } => cmd_config_proxy(url, no_proxy, clear, json).await,
            ConfigCommand::Game { args } => cmd_config_game(args, json).await,
            ConfigCommand::ClearGames { yes } => cmd_config_clear_games(yes, json).await,
        },
        #[cfg(feature = "cloud")]
        Command::Cloud { command } => match command {
            CloudCommand::Sync {
                app_id,
                up,
                down,
                path,
                resolve,
            } => {
                let path = path.map(|p| crate::output::user_path(&p));
                cmd_cloud_sync(app_id, up, down, path, resolve, json).await
            }
            CloudCommand::List { app_id } => cmd_cloud_list(app_id, json).await,
        },
        #[cfg(feature = "workshop")]
        Command::Workshop { command } => match command {
            WorkshopCommand::Browse {
                app_id,
                search,
                sort,
                count,
                cursor,
                tags,
            } => cmd_workshop_browse(app_id, search, sort, count, cursor, tags, json).await,
            WorkshopCommand::Info { ids } => cmd_workshop_info(ids, json).await,
            WorkshopCommand::List { app_id } => cmd_workshop_list(app_id, json).await,
            WorkshopCommand::Install { ids, no_recurse } => {
                cmd_workshop_install(ids, no_recurse, json).await
            }
            WorkshopCommand::Uninstall { ids, no_recurse } => {
                cmd_workshop_uninstall(ids, no_recurse, json).await
            }
            WorkshopCommand::Subscribe {
                ids,
                install,
                no_recurse,
            } => cmd_workshop_subscribe(ids, install, no_recurse, json).await,
            WorkshopCommand::Unsubscribe { ids, no_recurse } => {
                cmd_workshop_unsubscribe(ids, no_recurse, json).await
            }
            WorkshopCommand::Status { app_id } => cmd_workshop_status(app_id, json).await,
            WorkshopCommand::Rate { id, vote } => {
                cmd_workshop_rate(id, matches!(vote, VoteArg::Up), json).await
            }
            WorkshopCommand::Comments { id, count, start } => {
                cmd_workshop_comments_read(id, start, count, json).await
            }
            WorkshopCommand::Comment { id, text } => cmd_workshop_comment(id, text, json).await,
        },
        Command::Proton { command } => match command {
            ProtonCommand::List { installed } => cmd_proton_list(installed, json).await,
            #[cfg(feature = "install")]
            ProtonCommand::Install { version } => cmd_proton_install(version, json).await,
            #[cfg(feature = "install")]
            ProtonCommand::Uninstall { version } => cmd_proton_uninstall(version, json).await,
            ProtonCommand::Default { version } => cmd_proton_default(version, json).await,
        },
        Command::Profiles { command } => match command {
            ProfilesCommand::List => cmd_profiles_list(json),
            ProfilesCommand::Show { name } => cmd_profiles_show(&name, json),
        },
        #[cfg(feature = "authentication")]
        Command::Kill => cmd_kill(json),
        Command::Capabilities { path } => cmd_capabilities(&path, json),
        #[cfg(feature = "authentication")]
        Command::Daemon {
            command: Some(sub), ..
        } => match sub {
            DaemonCommand::Stop { pid } => cmd_daemon_stop(pid, json),
            DaemonCommand::List => cmd_daemon_list(json),
        },
        // Bare `daemon` is intercepted in `async_main`; it never reaches here.
        #[cfg(feature = "authentication")]
        Command::Daemon { command: None, .. } => {
            bail!("`gaki-cli daemon` cannot be run as a forwarded command")
        }
    }
}

/// Discover Proton/Wine runtimes under the user's Steam directories.
#[cfg(test)]
mod tests;
