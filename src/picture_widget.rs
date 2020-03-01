
use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf;

use crate::shaders;
use crate::util;

use crate::playback_manager::*;

use gelatin::cgmath::{Matrix4, Vector3};
use gelatin::glium::glutin::event::{ElementState, MouseButton};
use gelatin::glium::{Display, Program, program, uniform, Frame, Surface, texture::SrgbTexture2d, texture::RawImage2d};
use gelatin::image::{self, ImageError, RgbaImage};

use gelatin::add_common_widget_functions;
use gelatin::window::Window;
use gelatin::misc::{Alignment, Length, LogicalRect, LogicalVector, WidgetPlacement};
use gelatin::{DrawContext, Event, EventKind, Widget, WidgetData, WidgetError};

use std::time::{Duration, Instant};

enum Picture {
    LoadRequested(PathBuf),
    Ready(Rc<SrgbTexture2d>),
}
impl Picture {
    fn texture(&mut self, display: &Display) -> Result<Rc<SrgbTexture2d>, ImageError> {
        self.upload_to_texture(display)?;
        if let Picture::Ready(texture) = self {
            Ok(texture.clone())
        } else {
            unreachable!()
        }
    }
    fn upload_to_texture(&mut self, display: &Display) -> Result<(), ImageError> {
        let mut tmp_picture = Picture::LoadRequested("".into());
        std::mem::swap(self, &mut tmp_picture);
        match tmp_picture {
            Picture::LoadRequested(path) => {
                let img = image::open(path)?;
                let rgba = img.into_rgba();
                *self = Picture::Ready(Rc::new(Self::cpu_to_texture(rgba, display)));
            }
            Picture::Ready(texture) => {
                // This must be done because `img` was taken from `borrowed` when
                // `borrowed` was swapped with `tmp_picture`.
                *self = Picture::Ready(texture);
            }
        };
        Ok(())
    }

    fn cpu_to_texture(img: RgbaImage, display: &Display) -> SrgbTexture2d {
        let image_dimensions = img.dimensions();
        let image = RawImage2d::from_raw_rgba(img.into_raw(), image_dimensions);
        SrgbTexture2d::with_mipmaps(
            display, image, gelatin::glium::texture::MipmapsOption::AutoGeneratedMipmaps
        ).unwrap()
    }
}
struct PictureWidgetData {
    pub placement: WidgetPlacement,
    pub drawn_bounds: LogicalRect,

    pub click: bool,
    pub hover: bool,
    pub image_texture: Option<Picture>,

    playback_manager: PlaybackManager,

    program: Program,
    img_texel_size: f32,
    image_fit: bool,
    img_pos: LogicalVector,

    last_click_time: Instant,
    last_mouse_pos: LogicalVector,
    panning: bool,
    moving_window: bool,

    pub rendered_valid: bool,
}
impl WidgetData for PictureWidgetData {
    fn placement(&mut self) -> &mut WidgetPlacement {
        &mut self.placement
    }
    fn drawn_bounds(&mut self) -> &mut LogicalRect {
        &mut self.drawn_bounds
    }
}
impl PictureWidgetData {
    fn fit_image_to_panel(&mut self, display: &Display) -> Option<Rc<SrgbTexture2d>> {
        let size = self.drawn_bounds.size.vec;
        if let Some(texture) = self.get_texture(display) {
            let panel_aspect = size.x / size.y;
            let img_aspect = texture.width() as f32 / texture.height() as f32;

            let texel_size_to_fit_width = size.x / texture.width() as f32;
            let img_texel_size = if img_aspect > panel_aspect {
                // The image is relatively wider than the panel
                texel_size_to_fit_width
            } else {
                texel_size_to_fit_width * (img_aspect / panel_aspect)
            };
            self.img_pos = LogicalVector::new(
                size.x as f32 * 0.5,
                size.y as f32 * 0.5,
            );
            self.img_texel_size = img_texel_size;
            self.image_fit = true;
            Some(texture)
        } else {
            None
        }
    }

    fn pause_playback(window: &Window, playback_manager: &mut PlaybackManager) {
        playback_manager.pause_playback();
        let filename = playback_manager
            .current_filename()
            .to_str()
            .unwrap()
            .to_owned();
        window.set_title_filename(filename.as_ref());
    }

    fn toggle_playback(&mut self, window: &Window, playback_manager: &mut PlaybackManager) {
        match playback_manager.playback_state() {
            PlaybackState::Forward => Self::pause_playback(window, playback_manager),
            PlaybackState::Paused => {
                playback_manager.start_playback_forward();
                window.set_title_filename("Playing");
            }
            _ => (),
        }
    }

    fn get_texture(&mut self, display: &Display) -> Option<Rc<SrgbTexture2d>> {
        if let Some(ref mut tex) = self.image_texture {
            match tex.texture(display) {
                Ok(img) => Some(img),
                Err(err) => {
                    self.image_texture = None;
                    eprintln!("Can't load image: {}", err);
                    None
                }
            }
        } else {
            None
        }
    }
}

pub struct PictureWidget {
    data: RefCell<PictureWidgetData>,
}
impl PictureWidget {
    pub fn new(display: &Display) -> PictureWidget {
        let program = program!(display,
            140 => {
                vertex: shaders::VERTEX_140,
                fragment: shaders::FRAGMENT_140
            },

            110 => {
                vertex: shaders::VERTEX_110,
                fragment: shaders::FRAGMENT_110
            },
        )
        .unwrap();
        PictureWidget {
            data: RefCell::new(PictureWidgetData {
                placement: Default::default(),
                click: false,
                hover: false,
                image_texture: None,
                playback_manager: PlaybackManager::new(),
                drawn_bounds: Default::default(),
                rendered_valid: false,

                program,
                img_texel_size: 0.0,
                image_fit: true,
                img_pos: Default::default(),
                last_click_time: Instant::now() - Duration::from_secs(10),
                last_mouse_pos: Default::default(),
                panning: false,
                moving_window: false,
            }),
        }
    }

    add_common_widget_functions!(data);
}

impl Widget for PictureWidget {
    fn is_valid(&self) -> bool {
        self.data.borrow().rendered_valid
    }

    fn before_draw(&self, window: &Window) {
        let mut data = self.data.borrow_mut();
        data.playback_manager.update_image(window);
    }

    fn draw(&self, target: &mut Frame, context: &DrawContext) -> Result<(), WidgetError> {
        let texture;
        {
            let mut data = self.data.borrow_mut();
            if data.image_fit {
                texture = data.fit_image_to_panel(context.display);
            } else {
                texture = data.get_texture(context.display)
            }
        }
        {
            let data = self.data.borrow();

            let size = data.drawn_bounds.size.vec;
            let projection_transform = gelatin::cgmath::ortho(0.0, size.x, size.y, 0.0, -1.0, 1.0);

            let image_draw_params = gelatin::glium::DrawParameters {
                viewport: Some(context.logical_rect_to_viewport(&data.drawn_bounds)),
                ..Default::default()
            };

            if let Some(texture) = texture {
                let img_w = texture.width() as f32;
                let img_h = texture.height() as f32;

                let img_height_over_width = img_h / img_w;
                let image_display_width = data.img_texel_size * img_w;

                // Model tranform
                let image_display_height = image_display_width * img_height_over_width;
                let corner_x = (data.img_pos.vec.x - image_display_width * 0.5).floor();
                let corner_y = (data.img_pos.vec.y - image_display_height * 0.5).floor();
                let transform =
                    Matrix4::from_nonuniform_scale(image_display_width, image_display_height, 1.0);
                let transform =
                    Matrix4::from_translation(Vector3::new(corner_x, corner_y, 0.0)) * transform;
                // Projection tranform
                let transform = projection_transform * transform;

                let sampler = texture
                    .sampled()
                    .wrap_function(gelatin::glium::uniforms::SamplerWrapFunction::Clamp);
                let sampler = if data.img_texel_size >= 4f32 {
                    sampler.magnify_filter(gelatin::glium::uniforms::MagnifySamplerFilter::Nearest)
                } else {
                    sampler.magnify_filter(gelatin::glium::uniforms::MagnifySamplerFilter::Linear)
                };
                // building the uniforms
                let light_theme = true;
                let uniforms = uniform! {
                    matrix: Into::<[[f32; 4]; 4]>::into(transform),
                    bright_shade: if light_theme { 0.95f32 } else { 0.3f32 },
                    tex: sampler
                };
                target
                    .draw(
                        context.unit_quad_vertices,
                        context.unit_quad_indices,
                        &data.program,
                        &uniforms,
                        &image_draw_params,
                    )
                    .unwrap();
            }
        }
        self.data.borrow_mut().rendered_valid = true;
        Ok(())
    }

    fn layout(&self, available_space: LogicalRect) {
        let mut borrowed = self.data.borrow_mut();
        borrowed.default_layout(available_space);
    }

    fn handle_event(&self, event: &Event) {
        match event.kind {
            EventKind::MouseMove => {
                let mut borrowed = self.data.borrow_mut();
                borrowed.hover = borrowed.drawn_bounds.contains(event.cursor_pos);
            }
            EventKind::MouseButton { state, button: MouseButton::Left, .. } => {
                let mut borrowed = self.data.borrow_mut();
                match state {
                    ElementState::Pressed => {
                        borrowed.click = borrowed.hover;
                    }
                    ElementState::Released => {
                        borrowed.click = false;
                    }
                }
                borrowed.rendered_valid = false;
            },
            EventKind::DroppedFile(ref path) => {
                let mut borrowed = self.data.borrow_mut();
                borrowed.image_texture = Some(Picture::LoadRequested(path.clone()));
                borrowed.rendered_valid = false;
            }
            _ => (),
        }
    }

    // No children for a button
    fn children(&self, _children: &mut Vec<Rc<dyn Widget>>) {}

    fn placement(&self) -> WidgetPlacement {
        self.data.borrow().placement
    }
}
