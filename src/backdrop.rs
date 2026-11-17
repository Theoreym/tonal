//! Two-pass GPU blur of the already painted native interface, before the modal is drawn.
use eframe::{egui, egui_glow, glow};
use glow::HasContext;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Backdrop {
    gpu: Arc<Mutex<Option<Gpu>>>,
    failed: Arc<Mutex<bool>>,
}
impl Backdrop {
    pub fn paint(&self, ctx: &egui::Context, layer: egui::LayerId) {
        let gpu = self.gpu.clone();
        let failed = self.failed.clone();
        ctx.layer_painter(layer).add(egui::PaintCallback {
            rect: ctx.content_rect(),
            callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
                let mut gpu = gpu.lock().unwrap();
                if gpu.is_none() && !*failed.lock().unwrap() {
                    match unsafe { Gpu::new(painter.gl()) } {
                        Ok(created) => *gpu = Some(created),
                        Err(_) => {
                            *failed.lock().unwrap() = true;
                            eprintln!("Settings blur unavailable; using dimmed backdrop.");
                        }
                    }
                }
                if let Some(gpu) = gpu.as_mut() {
                    unsafe {
                        gpu.paint(
                            painter.gl(),
                            painter.intermediate_fbo(),
                            info.screen_size_px,
                        );
                    }
                }
            })),
        });
    }
    pub fn destroy(&self, gl: &glow::Context) {
        if let Some(gpu) = self.gpu.lock().unwrap().take() {
            unsafe {
                gpu.destroy(gl);
            }
        }
    }
}
struct Gpu {
    program: glow::Program,
    vao: glow::VertexArray,
    textures: [glow::Texture; 2],
    buffers: [glow::Framebuffer; 2],
    size: [i32; 2],
    direction: Option<glow::UniformLocation>,
}
impl Gpu {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        // All resources are created while the renderer's context is current.
        let program = gl.create_program()?;
        for (kind, source) in [
            (glow::VERTEX_SHADER, VERTEX),
            (glow::FRAGMENT_SHADER, FRAGMENT),
        ] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                gl.delete_shader(shader);
                gl.delete_program(program);
                return Err("Blur shader unavailable".into());
            }
            gl.attach_shader(program, shader);
            gl.delete_shader(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            gl.delete_program(program);
            return Err("Blur program unavailable".into());
        }
        let vao = gl.create_vertex_array()?;
        let textures = [gl.create_texture()?, gl.create_texture()?];
        let buffers = [gl.create_framebuffer()?, gl.create_framebuffer()?];
        for texture in textures {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            for (key, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, key, value as i32);
            }
        }
        let direction = gl.get_uniform_location(program, "direction");
        gl.use_program(Some(program));
        gl.uniform_1_i32(gl.get_uniform_location(program, "background").as_ref(), 0);
        Ok(Self {
            program,
            vao,
            textures,
            buffers,
            size: [0, 0],
            direction,
        })
    }
    unsafe fn paint(
        &mut self,
        gl: &glow::Context,
        target: Option<glow::Framebuffer>,
        screen: [u32; 2],
    ) {
        let width = screen[0] as i32;
        let height = screen[1] as i32;
        if width <= 0 || height <= 0 {
            return;
        }
        let size = [(width / 3).max(1), (height / 3).max(1)];
        gl.active_texture(glow::TEXTURE0);
        if self.size != size {
            self.size = size;
            for index in 0..2 {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.textures[index]));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    size[0],
                    size[1],
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.buffers[index]));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(self.textures[index]),
                    0,
                );
            }
        }
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, target);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.buffers[0]));
        gl.blit_framebuffer(
            0,
            0,
            width,
            height,
            0,
            0,
            size[0],
            size[1],
            glow::COLOR_BUFFER_BIT,
            glow::LINEAR,
        );
        gl.use_program(Some(self.program));
        gl.bind_vertex_array(Some(self.vao));
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.buffers[1]));
        gl.viewport(0, 0, size[0], size[1]);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.textures[0]));
        gl.uniform_2_f32(self.direction.as_ref(), 1.6 / size[0] as f32, 0.0);
        gl.draw_arrays(glow::TRIANGLES, 0, 3);
        gl.bind_framebuffer(glow::FRAMEBUFFER, target);
        gl.viewport(0, 0, width, height);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.textures[1]));
        gl.uniform_2_f32(self.direction.as_ref(), 0.0, 1.6 / size[1] as f32);
        gl.draw_arrays(glow::TRIANGLES, 0, 3);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.bind_vertex_array(None);
        // egui_glow restores blend, scissor, viewport and its own shader after this callback.
    }
    unsafe fn destroy(self, gl: &glow::Context) {
        for buffer in self.buffers {
            gl.delete_framebuffer(buffer);
        }
        for texture in self.textures {
            gl.delete_texture(texture);
        }
        gl.delete_vertex_array(self.vao);
        gl.delete_program(self.program);
    }
}
const VERTEX: &str = "#version 150\nout vec2 uv;\nvoid main() { vec2 p=vec2((gl_VertexID << 1) & 2, gl_VertexID & 2); uv=p; gl_Position=vec4(p*2.0-1.0,0.0,1.0); }";
const FRAGMENT: &str = "#version 150\nuniform sampler2D background; uniform vec2 direction; in vec2 uv; out vec4 color;\nvoid main() { vec3 c=texture(background,uv).rgb*0.227027; c+=(texture(background,uv+direction).rgb+texture(background,uv-direction).rgb)*0.194595; c+=(texture(background,uv+direction*2.0).rgb+texture(background,uv-direction*2.0).rgb)*0.121622; c+=(texture(background,uv+direction*3.0).rgb+texture(background,uv-direction*3.0).rgb)*0.054054; c+=(texture(background,uv+direction*4.0).rgb+texture(background,uv-direction*4.0).rgb)*0.016216; color=vec4(c,1.0); }";
