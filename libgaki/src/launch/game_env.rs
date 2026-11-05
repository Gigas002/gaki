//! The `GAKI_*` variables every launch exports, so a wrapper (a script in
//! `wrappers`) can see what it's wrapping. The command itself arrives as the
//! wrapper's arguments (`"$@"`), so `exec "$@"` runs the game.

use std::path::Path;

use crate::infra::runners::CommandSpec;

/// The Steam app id.
pub const ENV_APP_ID: &str = "GAKI_APP_ID";
/// The game's display name.
pub const ENV_APP_NAME: &str = "GAKI_APP_NAME";
/// The game's install directory, when known.
pub const ENV_GAME_DIR: &str = "GAKI_GAME_DIR";
/// The program the wrappers wrap.
pub const ENV_LAUNCH_PROGRAM: &str = "GAKI_LAUNCH_PROGRAM";
/// That program's arguments, space-joined.
pub const ENV_LAUNCH_ARGS: &str = "GAKI_LAUNCH_ARGS";
/// Native launches: the session file the game's stdout is captured to.
pub const ENV_STDOUT_LOG: &str = "GAKI_STDOUT_LOG";
/// Native launches: the session file the game's stderr is captured to.
pub const ENV_STDERR_LOG: &str = "GAKI_STDERR_LOG";

/// Add the `GAKI_*` variables for `spec`, before it is wrapped.
pub fn export(spec: &mut CommandSpec, app_id: u32, name: &str, game_dir: Option<&Path>) {
    let program = spec.program.to_string_lossy().into_owned();
    let args = spec.args.join(" ");
    let env = &mut spec.env;
    env.insert(ENV_APP_ID.into(), app_id.to_string());
    env.insert(ENV_APP_NAME.into(), name.to_owned());
    if let Some(dir) = game_dir {
        env.insert(ENV_GAME_DIR.into(), dir.to_string_lossy().into_owned());
    }
    env.insert(ENV_LAUNCH_PROGRAM.into(), program);
    env.insert(ENV_LAUNCH_ARGS.into(), args);
}
