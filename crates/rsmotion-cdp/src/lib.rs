pub mod browser;
pub mod client;
pub mod error;
pub mod handshake;
pub mod renderer;
pub mod server;
pub mod shm;

pub use browser::{discover_browser, BrowserProcess, GlRenderer};
pub use client::CdpClient;
pub use error::{CdpError, Result};
pub use renderer::{
    list_compositions_cdp, render_media_cdp, render_still_cdp, render_with_running_browser,
    CdpCompositionsOptions, CdpRenderOptions, CdpStillOptions, RenderReport, StillReport,
};
pub use server::EmbeddedServer;
