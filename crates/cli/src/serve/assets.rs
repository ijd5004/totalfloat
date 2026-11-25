//! The web app's files, compiled in from `ui/dist/` when it was built
//! (`cargo xtask ui`; see `build.rs`). Without them, the server shows its
//! page saying how to build the app.

use totalfloat_server::Server;

#[cfg(totalfloat_ui)]
mod built {
    include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));
}

/// The built app, served at `/`.
#[cfg(totalfloat_ui)]
struct Ui;

#[cfg(totalfloat_ui)]
impl totalfloat_server::Assets for Ui {
    fn get(&self, path: &str) -> Option<(&'static [u8], &'static str)> {
        built::FILES
            .iter()
            .find(|(name, _, _)| *name == path)
            .map(|(_, bytes, content_type)| (*bytes, *content_type))
    }
}

/// `server` with the built app, if this binary has one.
pub fn with_ui(server: Server) -> Server {
    #[cfg(totalfloat_ui)]
    let server = server.with_assets(Ui);
    server
}
