// Copyright (c) 2020-2021 Weird Constructor <weirdconstructor@gmail.com>
// This is a part of Kickmess. See README.md and COPYING for details.

use femtovg::{
    renderer::OpenGl,
    Canvas,
    FontId,
    ImageId,
    Color,
};

use crate::constants::*;
use crate::femtovg_painter::FemtovgPainter;

use raw_gl_context::{GlContext, GlConfig, Profile};

use raw_window_handle::{
    HasRawWindowHandle,
    RawWindowHandle
};

#[macro_use]
use baseview::{
    Size, Event, WindowEvent, MouseEvent, ScrollDelta, MouseButton, Window,
    WindowHandler, WindowOpenOptions, WindowScalePolicy,
};

use crate::{InputEvent, MButton, WindowUI};

struct FrameTimeMeasurement {
    buf: [u128; 240],
    idx: usize,
    cur: Option<std::time::Instant>,
    lbl: String,
}

impl FrameTimeMeasurement {
    fn new(lbl: &str) -> Self {
        Self {
            buf: [0; 240],
            idx: 0,
            cur: None,
            lbl: lbl.to_string(),
        }
    }

    fn start_measure(&mut self) {
        self.cur = Some(std::time::Instant::now());
    }

    fn end_measure(&mut self) {
        if let Some(cur) = self.cur.take() {
            let dur_microseconds = cur.elapsed().as_micros();
            if (self.idx + 1) >= self.buf.len() {
                let mut min = 99999999;
                let mut max = 0;
                let mut avg = 0;
                for b in self.buf.iter() {
                    if *b < min { min = *b; }
                    if *b > max { max = *b; }
                    avg += *b;
                }
                avg /= self.buf.len() as u128;
                println!("Frame time [{:10}]: min={:5.3}, max={:5.3}, avg={:5.3}", self.lbl, min as f64 / 1000.0, max as f64 / 1000.0, avg as f64 / 1000.0);
                self.idx = 0;
            } else {
                self.idx += 1;
            }
            self.buf[self.idx] = dur_microseconds;
        }
    }
}

pub struct GUIWindowHandler {
    context:    GlContext,
    canvas:     Canvas<OpenGl>,
    font:       FontId,
    font_mono:  FontId,
    img_buf:    ImageId,
    ftm:        FrameTimeMeasurement,
    ftm_redraw: FrameTimeMeasurement,
    ui:         Box<dyn WindowUI>,
    scale:      f32,
    size:       (f64, f64),
    focused:    bool,
    counter:    usize,
}

impl WindowHandler for GUIWindowHandler {

    fn on_event(&mut self, _: &mut Window, event: Event) {
        match event {
            Event::Mouse(MouseEvent::CursorMoved { position: p }) => {
                self.ui.handle_input_event(
                    InputEvent::MousePosition(
                        p.x / self.scale as f64,
                        p.y / self.scale as f64));
            },
            Event::Mouse(MouseEvent::ButtonPressed(btn)) => {
                let ev_btn =
                    match btn {
                        MouseButton::Left   => MButton::Left,
                        MouseButton::Right  => MButton::Right,
                        MouseButton::Middle => MButton::Middle,
                        _                   => MButton::Left,
                    };
                self.ui.handle_input_event(InputEvent::MouseButtonPressed(ev_btn));
            },
            Event::Mouse(MouseEvent::ButtonReleased(btn)) => {
                let ev_btn =
                    match btn {
                        MouseButton::Left   => MButton::Left,
                        MouseButton::Right  => MButton::Right,
                        MouseButton::Middle => MButton::Middle,
                        _                   => MButton::Left,
                    };
                self.ui.handle_input_event(InputEvent::MouseButtonReleased(ev_btn));
            },
            Event::Mouse(MouseEvent::WheelScrolled(scroll)) => {
                match scroll {
                    ScrollDelta::Lines { y, .. } => {
                        self.ui.handle_input_event(InputEvent::MouseWheel(y as f64));
                    },
                    ScrollDelta::Pixels { y, .. } => {
                        self.ui.handle_input_event(InputEvent::MouseWheel((y / 50.0) as f64));
                    },
                }
            },
            Event::Keyboard(ev) => {
                use keyboard_types::KeyState;
                match ev.state {
                    KeyState::Up => {
                        self.ui.handle_input_event(InputEvent::KeyReleased(ev));
                    },
                    KeyState::Down => {
                        self.ui.handle_input_event(InputEvent::KeyPressed(ev));
                    },
                }
            },
            Event::Window(WindowEvent::WillClose) => {
                self.ui.handle_input_event(InputEvent::WindowClose);
            },
            Event::Window(WindowEvent::Focused) => {
                self.focused = true;
            },
            Event::Window(WindowEvent::Unfocused) => {
                self.focused = false;
            },
            Event::Window(WindowEvent::Resized(info)) => {
                let size = info.logical_size();

                self.canvas.set_size(size.width as u32, size.height as u32, 1.0);
                let (w, h) = (self.canvas.width(), self.canvas.height());
                self.canvas.delete_image(self.img_buf);
                self.img_buf =
                    self.canvas.create_image_empty(
                        w as usize, h as usize,
                        femtovg::PixelFormat::Rgb8,
                        femtovg::ImageFlags::FLIP_Y).expect("making image buffer");

                let ri = (w as f32) / (h as f32);
                let rs = (self.size.0 as f32) / (self.size.1 as f32);

                if rs > ri {
                    self.scale = (w as f32) / (self.size.0 as f32);
                } else {
                    self.scale = (h as f32) / (self.size.1 as f32);
                }

                self.ui.set_window_size(w as f64, h as f64);
            },
            _ => {
                println!("UNHANDLED EVENT: {:?}", event);
            },
        }
    }

    fn on_frame(&mut self) {
        self.counter += 1;
        if self.counter % 500 == 0 {
//            println!("REDRAW.....");
            self.counter = 0;
        }

        // some hosts only stop calling idle(), so we stop the UI redraw here:
        if !self.ui.is_active() {
            return;
        }

        self.ui.pre_frame();
        let redraw = self.ui.needs_redraw();

        if redraw {
            self.ftm.start_measure();
        }

        if redraw {
            self.ftm_redraw.start_measure();
            self.canvas.set_render_target(femtovg::RenderTarget::Image(self.img_buf));
            self.canvas.save();
            self.canvas.scale(self.scale, self.scale);
            self.canvas.clear_rect(
                0, 0,
                self.canvas.width() as u32,
                self.canvas.height() as u32,
                Color::rgbf(
                    UI_GUI_BG_CLR.0 as f32,
                    UI_GUI_BG_CLR.1 as f32,
                    UI_GUI_BG_CLR.2 as f32));
            self.ui.draw(&mut FemtovgPainter {
                canvas:     &mut self.canvas,
                font:       self.font,
//                images:     vec![],
                font_mono:  self.font_mono,
                scale:      self.scale,
            });
            self.canvas.restore();
            self.ftm_redraw.end_measure();
        }

        let img_paint =
            femtovg::Paint::image(
                self.img_buf, 0.0, 0.0,
                self.canvas.width(),
                self.canvas.height(),
                0.0, 1.0);
        let mut path = femtovg::Path::new();
        path.rect(0.0, 0.0, self.canvas.width(), self.canvas.height());

        self.canvas.set_render_target(femtovg::RenderTarget::Screen);
        self.canvas.fill_path(&mut path, img_paint);

        self.canvas.flush();

        self.context.swap_buffers();

        if redraw {
            self.ftm.end_measure();
        }

        self.ui.post_frame();
    }
}

struct StupidWindowHandleHolder {
    handle: *mut ::std::ffi::c_void,
}

unsafe impl Send for StupidWindowHandleHolder { }

unsafe impl raw_window_handle::HasRawWindowHandle for StupidWindowHandleHolder {
    fn raw_window_handle(&self) -> RawWindowHandle {
        raw_window_handle_from_parent(self.handle)
    }
}

pub fn open_window(title: &str, window_width: i32, window_height: i32, parent: Option<*mut ::std::ffi::c_void>, factory: Box<dyn FnOnce() -> Box<dyn WindowUI> + Send>) {
    println!("*** OPEN WINDOW ***");
    let options =
        WindowOpenOptions {
            title:  title.to_string(),
            size:   Size::new(window_width as f64, window_height as f64),
            scale:  WindowScalePolicy::ScaleFactor(1.0),
        };

    let window_create_fun = move |win: &mut Window| {
        let context =
            GlContext::create(
                win,
                GlConfig {
                    version:       (3, 2),
                    profile:       Profile::Core,
                    red_bits:      8,
                    blue_bits:     8,
                    green_bits:    8,
                    alpha_bits:    0,
                    depth_bits:    24,
                    stencil_bits:  8,
                    samples:       None,
                    srgb:          true,
                    double_buffer: true,
                    vsync:         true,
                }).expect("GL context to be creatable");
        context.make_current();
        gl::load_with(|symbol| context.get_proc_address(symbol) as *const _);

        let renderer =
            OpenGl::new(|symbol| context.get_proc_address(symbol) as *const _)
                .expect("Cannot create renderer");

        let mut canvas = Canvas::new(renderer).expect("Cannot create canvas");
        canvas.set_size(window_width as u32, window_height as u32, 1.0);
        let font      = canvas.add_font_mem(std::include_bytes!("font.ttf")).expect("can load font");
        let font_mono = canvas.add_font_mem(std::include_bytes!("font_mono.ttf")).expect("can load font");
        let (w, h) = (canvas.width(), canvas.height());
        let img_buf =
            canvas.create_image_empty(
                w as usize, h as usize,
                femtovg::PixelFormat::Rgb8,
                femtovg::ImageFlags::FLIP_Y).expect("making image buffer");

        let mut ui = factory();

        ui.set_window_size(window_width as f64, window_height as f64);

        GUIWindowHandler {
            ui,
            size: (window_width as f64, window_height as f64),
            context,
            canvas,
            font,
            font_mono,
            img_buf,
            ftm:        FrameTimeMeasurement::new("img"),
            ftm_redraw: FrameTimeMeasurement::new("redraw"),
            scale:      1.0,
            focused:    false,
            counter:    0,
        }
    };

    if let Some(parent) = parent {
        let swhh = StupidWindowHandleHolder { handle: parent };
        Window::open_parented(&swhh, options, window_create_fun)
    } else {
        Window::open_blocking(options, window_create_fun)
    }
}

#[cfg(target_os = "macos")]
fn raw_window_handle_from_parent(
    parent: *mut ::std::ffi::c_void
) -> RawWindowHandle {
    use raw_window_handle::macos::MacOSHandle;
    use cocoa::base::id;
    use objc::{msg_send, sel, sel_impl};

    let ns_view = parent as id;

    let ns_window: id = unsafe {
        msg_send![ns_view, window]
    };

    RawWindowHandle::MacOS(MacOSHandle {
        ns_window: ns_window as *mut ::std::ffi::c_void,
        ns_view: ns_view as *mut ::std::ffi::c_void,
        ..MacOSHandle::empty()
    })
}


#[cfg(target_os = "windows")]
fn raw_window_handle_from_parent(
    parent: *mut ::std::ffi::c_void
) -> RawWindowHandle {
    use raw_window_handle::windows::WindowsHandle;

    RawWindowHandle::Windows(WindowsHandle {
        hwnd: parent,
        ..WindowsHandle::empty()
    })
}


#[cfg(target_os = "linux")]
fn raw_window_handle_from_parent(
    parent: *mut ::std::ffi::c_void
) -> RawWindowHandle {
    use raw_window_handle::unix::XcbHandle;

    RawWindowHandle::Xcb(XcbHandle {
        window: parent as u32,
        ..XcbHandle::empty()
    })
}
