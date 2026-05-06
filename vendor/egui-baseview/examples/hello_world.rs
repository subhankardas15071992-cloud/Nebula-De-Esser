use baseview::{Size, WindowOpenOptions, WindowScalePolicy};
use egui::Context;
use egui_baseview::{EguiWindow, GraphicsConfig, Queue};

fn main() {
    let settings = WindowOpenOptions {
        title: String::from("egui-baseview hello world"),
        size: Size::new(300.0, 110.0),
        scale: WindowScalePolicy::SystemScaleFactor,
        #[cfg(feature = "opengl")]
        gl_config: Some(Default::default()),
    };

    let state = ();

    EguiWindow::open_blocking(
        settings,
        GraphicsConfig::default(),
        state,
        |_egui_ctx: &Context, _queue: &mut Queue, _state: &mut ()| {},
        |egui_ctx: &Context, _queue: &mut Queue, _state: &mut ()| {
            egui::Window::new("egui-baseview hello world").show(egui_ctx, |ui| {
                ui.label("Hello World!");
            });
        },
    );
}
