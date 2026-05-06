use egui_glow::PainterError;
use thiserror::Error;

pub mod renderer;

#[derive(Error, Debug)]
pub enum OpenGlError {
    #[error("Failed to get baseview's GL context")]
    NoContext,
    #[error("Error occured when initializing painter: \n {0}")]
    CreatePainter(PainterError),
}
