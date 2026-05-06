#[cfg(feature = "opengl")]
mod opengl;
#[cfg(feature = "opengl")]
pub use opengl::renderer::{GraphicsConfig, Renderer};

#[cfg(feature = "wgpu")]
mod wgpu;
#[cfg(feature = "wgpu")]
pub use wgpu::renderer::{GraphicsConfig, Renderer};
