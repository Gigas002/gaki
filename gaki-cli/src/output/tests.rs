#[cfg(feature = "library")]
use super::*;

#[cfg(feature = "library")]
#[tokio::test]
async fn relative_paths_resolve_against_the_clients_directory() {
    use std::path::{Path, PathBuf};

    let absolute = Path::new("/srv/library.toml");
    assert_eq!(user_path(absolute), absolute);
    let here = std::env::current_dir().unwrap();
    assert_eq!(user_path(Path::new("lib.toml")), here.join("lib.toml"));

    let (out, _) = mpsc::unbounded_channel();
    let (_, stdin) = mpsc::unbounded_channel();
    let ctx = OutputCtx::new(out, stdin, Some(PathBuf::from("/home/me")));
    let (file, dir) = scoped(ctx, async {
        (user_path(Path::new("lib.toml")), user_path(absolute))
    })
    .await;
    assert_eq!(file, Path::new("/home/me/lib.toml"));
    assert_eq!(dir, absolute, "an absolute path is kept");
}
