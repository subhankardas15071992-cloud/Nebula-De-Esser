use std::{
    num::{NonZeroIsize, NonZeroU32},
    ptr::NonNull,
    sync::Arc,
};

use baseview::{PhySize, Window};
use egui::FullOutput;
use egui_wgpu::{
    wgpu::{
        Color, CommandEncoderDescriptor, Extent3d, Instance, InstanceDescriptor,
        RenderPassColorAttachment, RenderPassDescriptor, Surface, SurfaceConfiguration,
        SurfaceTargetUnsafe, TextureDescriptor, TextureDimension, TextureUsages, TextureView,
        TextureViewDescriptor,
    },
    RenderState, ScreenDescriptor, WgpuError,
};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use raw_window_handle_06::{
    AppKitDisplayHandle, AppKitWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
    XcbDisplayHandle, XcbWindowHandle, XlibDisplayHandle, XlibWindowHandle,
};

pub use egui_wgpu::WgpuConfiguration;

#[derive(Debug, Clone)]
pub struct GraphicsConfig {
    /// Controls whether to apply dithering to minimize banding artifacts.
    ///
    /// Dithering assumes an sRGB output and thus will apply noise to any input value that lies between
    /// two 8bit values after applying the sRGB OETF function, i.e. if it's not a whole 8bit value in "gamma space".
    /// This means that only inputs from texture interpolation and vertex colors should be affected in practice.
    ///
    /// Defaults to true.
    pub dithering: bool,

    /// Configures wgpu instance/device/adapter/surface creation and renderloop.
    pub wgpu_options: WgpuConfiguration,
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        Self {
            dithering: true,
            wgpu_options: Default::default(),
        }
    }
}

const MSAA_SAMPLES: u32 = 4;

pub struct Renderer {
    render_state: Arc<RenderState>,
    surface: Surface<'static>,
    config: GraphicsConfig,
    msaa_texture_view: Option<TextureView>,
    width: u32,
    height: u32,
}

impl Renderer {
    pub fn new(window: &Window, config: GraphicsConfig) -> Result<Self, WgpuError> {
        let instance = Instance::new(InstanceDescriptor::default());

        let raw_display_handle = window.raw_display_handle();
        let raw_window_handle = window.raw_window_handle();

        let target = SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: match raw_display_handle {
                raw_window_handle::RawDisplayHandle::AppKit(_) => {
                    raw_window_handle_06::RawDisplayHandle::AppKit(AppKitDisplayHandle::new())
                }
                raw_window_handle::RawDisplayHandle::Xlib(handle) => {
                    raw_window_handle_06::RawDisplayHandle::Xlib(XlibDisplayHandle::new(
                        NonNull::new(handle.display),
                        handle.screen,
                    ))
                }
                raw_window_handle::RawDisplayHandle::Xcb(handle) => {
                    raw_window_handle_06::RawDisplayHandle::Xcb(XcbDisplayHandle::new(
                        NonNull::new(handle.connection),
                        handle.screen,
                    ))
                }
                raw_window_handle::RawDisplayHandle::Windows(_) => {
                    raw_window_handle_06::RawDisplayHandle::Windows(WindowsDisplayHandle::new())
                }
                _ => todo!(),
            },
            raw_window_handle: match raw_window_handle {
                raw_window_handle::RawWindowHandle::AppKit(handle) => {
                    raw_window_handle_06::RawWindowHandle::AppKit(AppKitWindowHandle::new(
                        NonNull::new(handle.ns_view).unwrap(),
                    ))
                }
                raw_window_handle::RawWindowHandle::Xlib(handle) => {
                    raw_window_handle_06::RawWindowHandle::Xlib(XlibWindowHandle::new(
                        handle.window,
                    ))
                }
                raw_window_handle::RawWindowHandle::Xcb(handle) => {
                    raw_window_handle_06::RawWindowHandle::Xcb(XcbWindowHandle::new(
                        NonZeroU32::new(handle.window).unwrap(),
                    ))
                }
                raw_window_handle::RawWindowHandle::Win32(handle) => {
                    // will this work? i have no idea!
                    let mut raw_handle =
                        Win32WindowHandle::new(NonZeroIsize::new(handle.hwnd as isize).unwrap());

                    raw_handle.hinstance = handle
                        .hinstance
                        .is_null()
                        .then(|| NonZeroIsize::new(handle.hinstance as isize).unwrap());

                    raw_window_handle_06::RawWindowHandle::Win32(raw_handle)
                }
                _ => todo!(),
            },
        };

        let surface = unsafe { instance.create_surface_unsafe(target) }.unwrap();

        let state = Arc::new(pollster::block_on(RenderState::create(
            &config.wgpu_options,
            &instance,
            &surface,
            None,
            MSAA_SAMPLES,
            config.dithering,
        ))?);

        Ok(Self {
            render_state: state,
            surface,
            config,
            msaa_texture_view: None,
            width: 0,
            height: 0,
        })
    }

    pub fn max_texture_side(&self) -> usize {
        self.render_state
            .as_ref()
            .device
            .limits()
            .max_texture_dimension_2d as usize
    }

    fn configure_surface(&self, width: u32, height: u32) {
        let usage = TextureUsages::RENDER_ATTACHMENT;

        let mut surf_config = SurfaceConfiguration {
            usage,
            format: self.render_state.target_format,
            present_mode: self.config.wgpu_options.present_mode,
            view_formats: vec![self.render_state.target_format],
            ..self
                .surface
                .get_default_config(&self.render_state.adapter, width, height)
                .expect("Unsupported surface")
        };

        if let Some(desired_maximum_frame_latency) =
            self.config.wgpu_options.desired_maximum_frame_latency
        {
            surf_config.desired_maximum_frame_latency = desired_maximum_frame_latency;
        }

        self.surface
            .configure(&self.render_state.device, &surf_config);
    }

    fn resize_and_generate_msaa_view(&mut self, width: u32, height: u32) {
        let render_state = self.render_state.as_ref();

        self.width = width;
        self.height = height;

        self.configure_surface(width, height);

        let texture_format = render_state.target_format;
        self.msaa_texture_view = Some(
            render_state
                .device
                .create_texture(&TextureDescriptor {
                    label: Some("egui_msaa_texture"),
                    size: Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: MSAA_SAMPLES,
                    dimension: TextureDimension::D2,
                    format: texture_format,
                    usage: TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[texture_format],
                })
                .create_view(&TextureViewDescriptor::default()),
        );
    }

    pub fn render(
        &mut self,
        bg_color: egui::Rgba,
        physical_size: PhySize,
        pixels_per_point: f32,
        egui_ctx: &mut egui::Context,
        full_output: &mut FullOutput,
    ) {
        let PhySize {
            width: canvas_width,
            height: canvas_height,
        } = physical_size;

        let shapes = std::mem::take(&mut full_output.shapes);

        let clipped_primitives = egui_ctx.tessellate(shapes, pixels_per_point);

        let mut encoder =
            self.render_state
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("encoder"),
                });

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [canvas_width, canvas_height],
            pixels_per_point,
        };

        let user_cmd_bufs = {
            let mut renderer = self.render_state.renderer.write();
            for (id, image_delta) in &full_output.textures_delta.set {
                renderer.update_texture(
                    &self.render_state.device,
                    &self.render_state.queue,
                    *id,
                    image_delta,
                );
            }

            renderer.update_buffers(
                &self.render_state.device,
                &self.render_state.queue,
                &mut encoder,
                &clipped_primitives,
                &screen_descriptor,
            )
        };

        if self.width != canvas_width
            || self.height != canvas_height
            || self.msaa_texture_view.is_none()
        {
            self.resize_and_generate_msaa_view(canvas_width, canvas_height);
        }

        let output_frame = { self.surface.get_current_texture() };

        let output_frame = match output_frame {
            Ok(frame) => frame,
            Err(err) => match (self.config.wgpu_options.on_surface_error)(err) {
                egui_wgpu::SurfaceErrorAction::SkipFrame => return,
                egui_wgpu::SurfaceErrorAction::RecreateSurface => {
                    self.configure_surface(self.width, self.height);
                    return;
                }
            },
        };

        {
            let renderer = self.render_state.renderer.read();
            let frame_view = output_frame
                .texture
                .create_view(&TextureViewDescriptor::default());

            let render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("egui_render"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: self.msaa_texture_view.as_ref().unwrap(),
                    resolve_target: Some(&frame_view),
                    ops: egui_wgpu::wgpu::Operations {
                        load: egui_wgpu::wgpu::LoadOp::Clear(Color {
                            r: bg_color[0] as f64,
                            g: bg_color[1] as f64,
                            b: bg_color[2] as f64,
                            a: bg_color[3] as f64,
                        }),
                        store: egui_wgpu::wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Forgetting the pass' lifetime means that we are no longer compile-time protected from
            // runtime errors caused by accessing the parent encoder before the render pass is dropped.
            // Since we don't pass it on to the renderer, we should be perfectly safe against this mistake here!
            renderer.render(
                &mut render_pass.forget_lifetime(),
                &clipped_primitives,
                &screen_descriptor,
            );
        }

        {
            let mut renderer = self.render_state.renderer.write();
            for id in &full_output.textures_delta.free {
                renderer.free_texture(id);
            }
        }

        let encoded = encoder.finish();

        self.render_state
            .queue
            .submit(user_cmd_bufs.into_iter().chain([encoded]));

        output_frame.present();
    }
}
