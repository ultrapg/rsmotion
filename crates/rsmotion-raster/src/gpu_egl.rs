use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;

use khronos_egl as egl;
use libloading::Library;
use serde::{Deserialize, Serialize};
use skia_safe::runtime_effect::{ChildPtr, Options as RuntimeEffectOptions};
use skia_safe::{
    gpu, AlphaType, BlendMode, ColorType, Data, FilterMode, ImageInfo, MipmapMode, Paint,
    RuntimeEffect, SamplingOptions, Surface, TileMode,
};
use tracing::{debug, warn};

use crate::error::{RasterError, Result};

const EGL_PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;
const EGL_PLATFORM_DEVICE_EXT: egl::Enum = 0x313F;
const EGL_OPENGL_ES3_BIT: egl::Int = 0x0040;
const GL_VENDOR: u32 = 0x1F00;
const GL_RENDERER: u32 = 0x1F01;
const GL_VERSION: u32 = 0x1F02;

const PACKED_NV12_SKSL: &str = r#"
uniform shader src;
uniform float u_height;

float calc_y(half4 c) {
    float r = clamp(float(c.r), 0.0, 1.0) * 255.0;
    float g = clamp(float(c.g), 0.0, 1.0) * 255.0;
    float b = clamp(float(c.b), 0.0, 1.0) * 255.0;
    float y = floor((66.0 * r + 129.0 * g + 25.0 * b + 128.0) / 256.0) + 16.0;
    return clamp(y, 16.0, 235.0) / 255.0;
}

float2 calc_uv(float x, float y) {
    half4 p00 = src.eval(float2(x, y));
    half4 p01 = src.eval(float2(x + 1.0, y));
    half4 p10 = src.eval(float2(x, y + 1.0));
    half4 p11 = src.eval(float2(x + 1.0, y + 1.0));
    float r = floor((float(p00.r + p01.r + p10.r + p11.r) * 255.0 + 2.0) * 0.25);
    float g = floor((float(p00.g + p01.g + p10.g + p11.g) * 255.0 + 2.0) * 0.25);
    float b = floor((float(p00.b + p01.b + p10.b + p11.b) * 255.0 + 2.0) * 0.25);
    float u = floor((-38.0 * r - 74.0 * g + 112.0 * b + 128.0) / 256.0) + 128.0;
    float v = floor((112.0 * r - 94.0 * g - 18.0 * b + 128.0) / 256.0) + 128.0;
    return float2(clamp(u, 16.0, 240.0) / 255.0, clamp(v, 16.0, 240.0) / 255.0);
}

half4 main(float2 coord) {
    float tx = floor(coord.x);
    float ty = floor(coord.y);
    if (ty < u_height) {
        float x0 = tx * 4.0 + 0.5;
        float py = ty + 0.5;
        float y0 = calc_y(src.eval(float2(x0, py)));
        float y1 = calc_y(src.eval(float2(x0 + 1.0, py)));
        float y2 = calc_y(src.eval(float2(x0 + 2.0, py)));
        float y3 = calc_y(src.eval(float2(x0 + 3.0, py)));
        return half4(half(y0), half(y1), half(y2), half(y3));
    } else {
        float x0 = tx * 4.0 + 0.5;
        float sy = (ty - u_height) * 2.0 + 0.5;
        float2 uv0 = calc_uv(x0, sy);
        float2 uv1 = calc_uv(x0 + 2.0, sy);
        return half4(half(uv0.x), half(uv0.y), half(uv1.x), half(uv1.y));
    }
}
"#;

/// Metadata proving which OpenGL ES / EGL GPU driver backed the Skia context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    pub egl_platform: String,
    pub vendor: String,
    pub renderer: String,
    pub version: String,
}

struct GpuNv12Pipeline {
    packed_surface: Surface,
    packed_effect: RuntimeEffect,
    packed_info: ImageInfo,
    height_uniform: Data,
    row_bytes: usize,
}

/// Headless EGL + OpenGL ES 3.0 + Skia Ganesh GPU offscreen render surface.
pub struct HeadlessGpuSurface {
    // Field drop order matters: Skia Surfaces & DirectContext must drop BEFORE EGL context.
    surface: Surface,
    nv12_pipeline: Option<GpuNv12Pipeline>,
    direct_context: gpu::DirectContext,
    info: GpuInfo,
    egl: egl::DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    egl_surface: Option<egl::Surface>,
    egl_context: egl::Context,
    _gles_lib: Option<Library>,
}

fn query_gl_string(
    egl: &egl::DynamicInstance<egl::EGL1_5>,
    gles_lib: Option<&Library>,
    enum_id: u32,
) -> String {
    let get_string_ptr = egl
        .get_proc_address("glGetString")
        .map(|f| f as *const c_void)
        .filter(|p| !p.is_null())
        .or_else(|| {
            gles_lib.and_then(|lib| unsafe {
                lib.get::<*const c_void>(b"glGetString\0")
                    .ok()
                    .map(|sym| *sym)
            })
        });

    let Some(ptr) = get_string_ptr else {
        return "<unavailable>".to_string();
    };
    let gl_get_string: unsafe extern "C" fn(u32) -> *const c_char =
        unsafe { std::mem::transmute(ptr) };
    let raw = unsafe { gl_get_string(enum_id) };
    if raw.is_null() {
        return "<null>".to_string();
    }
    unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned()
}

fn acquire_display(egl: &egl::DynamicInstance<egl::EGL1_5>) -> Result<(egl::Display, String)> {
    // 1. Mesa surfaceless EGL (preferred headless Linux path without X11/Wayland)
    if let Ok(dpy) = unsafe {
        egl.get_platform_display(
            EGL_PLATFORM_SURFACELESS_MESA,
            egl::DEFAULT_DISPLAY,
            &[egl::ATTRIB_NONE],
        )
    } {
        if dpy.as_ptr() != egl::NO_DISPLAY {
            return Ok((dpy, "EGL_PLATFORM_SURFACELESS_MESA".to_string()));
        }
    }

    // 2. Try EGL_EXT_device_base + EGL_PLATFORM_DEVICE_EXT if surfaceless unavailable
    if let Some(query_devices_raw) = egl.get_proc_address("eglQueryDevicesEXT") {
        type QueryDevicesFn = unsafe extern "C" fn(i32, *mut *mut c_void, *mut i32) -> u32;
        let query_devices: QueryDevicesFn = unsafe { std::mem::transmute(query_devices_raw) };
        let mut devices = [std::ptr::null_mut(); 8];
        let mut num_devices = 0i32;
        if unsafe { query_devices(8, devices.as_mut_ptr(), &mut num_devices) } != 0
            && num_devices > 0
        {
            for &dev in &devices[..(num_devices as usize)] {
                if dev.is_null() {
                    continue;
                }
                if let Ok(dpy) = unsafe {
                    egl.get_platform_display(EGL_PLATFORM_DEVICE_EXT, dev, &[egl::ATTRIB_NONE])
                } {
                    if dpy.as_ptr() != egl::NO_DISPLAY {
                        return Ok((dpy, "EGL_PLATFORM_DEVICE_EXT".to_string()));
                    }
                }
            }
        }
    }

    // 3. Fallback default display
    if let Some(dpy) = unsafe { egl.get_display(egl::DEFAULT_DISPLAY) } {
        if dpy.as_ptr() != egl::NO_DISPLAY {
            return Ok((dpy, "EGL_DEFAULT_DISPLAY".to_string()));
        }
    }

    Err(RasterError::GpuUnavailable(
        "could not obtain any headless EGL display".to_string(),
    ))
}

fn try_create_nv12_pipeline(
    direct_context: &mut gpu::DirectContext,
    width: u32,
    height: u32,
) -> Option<GpuNv12Pipeline> {
    if width < 4 || height < 2 || !width.is_multiple_of(4) || !height.is_multiple_of(2) {
        return None;
    }
    let pack_w = (width / 4) as i32;
    let pack_h = (height + height / 2) as i32;
    let packed_info = ImageInfo::new(
        (pack_w, pack_h),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );

    let packed_surface = match gpu::surfaces::render_target(
        direct_context,
        gpu::Budgeted::No,
        &packed_info,
        Some(0),
        gpu::SurfaceOrigin::TopLeft,
        None,
        false,
        false,
    ) {
        Some(s) => s,
        None => {
            warn!("GPU packed RGBA8888 Unpremul NV12 surface unsupported; using RGBA readback");
            return None;
        }
    };

    let opts = RuntimeEffectOptions::default();
    let packed_effect = match RuntimeEffect::make_for_shader(PACKED_NV12_SKSL, Some(&opts)) {
        Ok(e) => e,
        Err(err) => {
            warn!("SkSL packed NV12 shader compile failed ({err}); using RGBA readback");
            return None;
        }
    };

    let height_uniform = Data::new_copy(&(height as f32).to_ne_bytes());
    Some(GpuNv12Pipeline {
        packed_surface,
        packed_effect,
        packed_info,
        height_uniform,
        row_bytes: width as usize,
    })
}

impl HeadlessGpuSurface {
    /// Creates a headless hardware-accelerated EGL/GLES3 Skia surface at `width x height`.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let egl = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|e| RasterError::GpuUnavailable(format!("failed to load libEGL.so.1: {e}")))?;

        let (display, egl_platform) = acquire_display(&egl)?;
        let (_major, _minor) = egl.initialize(display).map_err(|e| {
            RasterError::GpuUnavailable(format!("eglInitialize failed on {egl_platform}: {e}"))
        })?;

        egl.bind_api(egl::OPENGL_ES_API)
            .map_err(|e| RasterError::GpuUnavailable(format!("eglBindAPI(GLES) failed: {e}")))?;

        let es3_attribs = [
            egl::SURFACE_TYPE,
            egl::PBUFFER_BIT,
            egl::RENDERABLE_TYPE,
            EGL_OPENGL_ES3_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::NONE,
        ];
        let es2_attribs = [
            egl::SURFACE_TYPE,
            egl::PBUFFER_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES2_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::NONE,
        ];
        let config = match egl
            .choose_first_config(display, &es3_attribs)
            .map_err(|e| RasterError::GpuUnavailable(format!("eglChooseConfig error: {e}")))?
        {
            Some(c) => c,
            None => egl
                .choose_first_config(display, &es2_attribs)
                .map_err(|e| RasterError::GpuUnavailable(format!("eglChooseConfig error: {e}")))?
                .ok_or_else(|| {
                    RasterError::GpuUnavailable("no matching GLES RGBA8 EGLConfig".to_string())
                })?,
        };

        let ctx_attribs = [egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE];
        let egl_context = egl
            .create_context(display, config, None, &ctx_attribs)
            .map_err(|e| {
                RasterError::GpuUnavailable(format!("eglCreateContext(GLES3) failed: {e}"))
            })?;

        let mut egl_surface = None;
        if let Err(e) = egl.make_current(display, None, None, Some(egl_context)) {
            debug!("EGL_NO_SURFACE make_current failed ({e}), trying 1x1 pbuffer fallback");
            let pb_attribs = [egl::WIDTH, 1, egl::HEIGHT, 1, egl::NONE];
            let pbuffer = egl
                .create_pbuffer_surface(display, config, &pb_attribs)
                .map_err(|pe| {
                    RasterError::GpuUnavailable(format!(
                        "surfaceless failed ({e}) and pbuffer failed ({pe})"
                    ))
                })?;
            egl.make_current(display, Some(pbuffer), Some(pbuffer), Some(egl_context))
                .map_err(|me| {
                    RasterError::GpuUnavailable(format!("eglMakeCurrent(pbuffer) failed: {me}"))
                })?;
            egl_surface = Some(pbuffer);
        }

        let gles_lib = unsafe { Library::new("libGLESv2.so.2") }.ok();

        let vendor = query_gl_string(&egl, gles_lib.as_ref(), GL_VENDOR);
        let renderer = query_gl_string(&egl, gles_lib.as_ref(), GL_RENDERER);
        let version = query_gl_string(&egl, gles_lib.as_ref(), GL_VERSION);

        let renderer_lower = renderer.to_lowercase();
        if renderer_lower.contains("llvmpipe")
            || renderer_lower.contains("softpipe")
            || renderer_lower.contains("swrast")
        {
            let _ = egl.make_current(display, None, None, None);
            if let Some(s) = egl_surface {
                let _ = egl.destroy_surface(display, s);
            }
            let _ = egl.destroy_context(display, egl_context);
            let _ = egl.terminate(display);
            return Err(RasterError::GpuUnavailable(format!(
                "EGL initialized software renderer `{renderer}` instead of hardware GPU"
            )));
        }

        let interface = gpu::gl::Interface::new_load_with(|name| {
            if let Some(f) = egl.get_proc_address(name) {
                let p = f as *const c_void;
                if !p.is_null() {
                    return p;
                }
            }
            if let (Some(lib), Ok(cname)) = (gles_lib.as_ref(), CString::new(name)) {
                if let Ok(sym) = unsafe { lib.get::<*const c_void>(cname.as_bytes_with_nul()) } {
                    return *sym;
                }
            }
            std::ptr::null()
        })
        .ok_or_else(|| {
            RasterError::SkiaFailed("failed to assemble Skia GL interface".to_string())
        })?;

        let mut direct_context =
            gpu::direct_contexts::make_gl(interface, None).ok_or_else(|| {
                RasterError::SkiaFailed("DirectContext::make_gl returned None".to_string())
            })?;

        let image_info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );

        let surface = gpu::surfaces::render_target(
            &mut direct_context,
            gpu::Budgeted::No,
            &image_info,
            Some(0),
            gpu::SurfaceOrigin::TopLeft,
            None,
            false,
            false,
        )
        .ok_or_else(|| {
            RasterError::SkiaFailed("gpu::surfaces::render_target returned None".to_string())
        })?;

        let nv12_pipeline = try_create_nv12_pipeline(&mut direct_context, width, height);

        Ok(Self {
            surface,
            nv12_pipeline,
            direct_context,
            info: GpuInfo {
                egl_platform,
                vendor,
                renderer,
                version,
            },
            egl,
            display,
            egl_surface,
            egl_context,
            _gles_lib: gles_lib,
        })
    }

    pub fn info(&self) -> &GpuInfo {
        &self.info
    }

    pub fn canvas(&mut self) -> &skia_safe::Canvas {
        self.surface.canvas()
    }

    /// Whether GPU-side SkSL RGBA->NV12 readback (`Alpha8` + `R8G8UNorm`) is active.
    pub fn supports_nv12(&self) -> bool {
        self.nv12_pipeline.is_some()
    }

    /// Flushes and submits GPU commands to OpenGL ES.
    pub fn flush_and_submit(&mut self) -> Result<()> {
        self.direct_context
            .flush_and_submit()
            .map_err(|e| RasterError::SkiaFailed(format!("flush_and_submit failed: {e:?}")))?;
        Ok(())
    }

    /// Reads back rendered RGBA8888 pixels into `dst`.
    pub fn read_pixels_rgba(&mut self, width: u32, height: u32, dst: &mut [u8]) -> Result<()> {
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let row_bytes = (width as usize) * 4;
        if !self.surface.read_pixels(&info, dst, row_bytes, (0, 0)) {
            return Err(RasterError::SkiaFailed(
                "GPU surface read_pixels failed".to_string(),
            ));
        }
        Ok(())
    }

    /// Converts the rendered RGBA surface on-GPU via a single packed RGBA8888 SkSL pass (`3.11 MB` at 1080p).
    pub fn read_pixels_nv12(&mut self, width: u32, height: u32, dst: &mut [u8]) -> Result<()> {
        let w = width as usize;
        let h = height as usize;
        let expected_len = (w * h * 3) / 2;
        if dst.len() != expected_len {
            return Err(RasterError::SkiaFailed(format!(
                "NV12 buffer length mismatch: expected {expected_len}, got {}",
                dst.len()
            )));
        }

        let snapshot = self.surface.image_snapshot();
        let src_shader = snapshot
            .to_shader(
                (TileMode::Clamp, TileMode::Clamp),
                SamplingOptions::new(FilterMode::Nearest, MipmapMode::None),
                None,
            )
            .ok_or_else(|| RasterError::SkiaFailed("NV12 snapshot to_shader failed".to_string()))?;

        let pipe = self.nv12_pipeline.as_mut().ok_or_else(|| {
            RasterError::SkiaFailed("GPU NV12 pipeline unavailable on surface".to_string())
        })?;

        let shader = pipe
            .packed_effect
            .make_shader(
                pipe.height_uniform.clone(),
                &[ChildPtr::Shader(src_shader)],
                None,
            )
            .ok_or_else(|| RasterError::SkiaFailed("packed NV12 make_shader failed".to_string()))?;
        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::Src);
        paint.set_shader(shader);
        pipe.packed_surface.canvas().draw_paint(&paint);

        self.direct_context
            .flush_and_submit()
            .map_err(|e| RasterError::SkiaFailed(format!("NV12 flush_and_submit failed: {e:?}")))?;

        if !pipe
            .packed_surface
            .read_pixels(&pipe.packed_info, dst, pipe.row_bytes, (0, 0))
        {
            return Err(RasterError::SkiaFailed(
                "GPU packed NV12 surface read_pixels failed".to_string(),
            ));
        }
        Ok(())
    }
}

impl Drop for HeadlessGpuSurface {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        if let Some(s) = self.egl_surface {
            let _ = self.egl.destroy_surface(self.display, s);
        }
        let _ = self.egl.destroy_context(self.display, self.egl_context);
        let _ = self.egl.terminate(display_handle_safe(self.display));
    }
}

fn display_handle_safe(display: egl::Display) -> egl::Display {
    display
}

#[cfg(test)]
mod tests {
    use rsmotion_encode::convert_rgba_or_bgra_to_nv12;
    use skia_safe::gradient::{
        shaders as gradient_shaders, Colors as GradientColors, Gradient, Interpolation,
    };
    use skia_safe::{Color, Color4f, Paint, Point, Rect};

    use super::*;

    #[test]
    fn gpu_sksl_nv12_matches_cpu_bt601() {
        let w = 64u32;
        let h = 64u32;
        let mut gpu = match HeadlessGpuSurface::new(w, h) {
            Ok(g) => g,
            Err(RasterError::GpuUnavailable(_)) => return,
            Err(e) => panic!("unexpected GPU init error: {e}"),
        };
        assert!(
            gpu.supports_nv12(),
            "Intel/Mesa GPU surface should support packed RGBA8888 NV12 readback"
        );

        let canvas = gpu.canvas();
        canvas.clear(Color::from_rgb(15, 23, 42));
        let colors = [
            Color4f::new(0.2, 0.7, 0.95, 1.0),
            Color4f::new(0.95, 0.35, 0.2, 1.0),
        ];
        let gcolors = GradientColors::new(&colors, None, TileMode::Clamp, None);
        let grad = Gradient::new(gcolors, Interpolation::default());
        if let Some(shader) = gradient_shaders::linear_gradient(
            (Point::new(0.0, 0.0), Point::new(64.0, 64.0)),
            &grad,
            None,
        ) {
            let mut paint = Paint::default();
            paint.set_shader(shader);
            canvas.draw_rect(Rect::from_xywh(8.0, 8.0, 48.0, 48.0), &paint);
        }
        // High-contrast vertical stripe inside a 4-pixel block so Y0..Y3 differ sharply (`max_diff` catches premul bugs).
        let mut stripe_paint = Paint::default();
        stripe_paint.set_color(Color::from_rgb(255, 255, 255));
        canvas.draw_rect(Rect::from_xywh(16.0, 16.0, 1.0, 16.0), &stripe_paint);
        stripe_paint.set_color(Color::from_rgb(0, 0, 0));
        canvas.draw_rect(Rect::from_xywh(19.0, 16.0, 1.0, 16.0), &stripe_paint);
        gpu.flush_and_submit().unwrap();

        let mut rgba = vec![0u8; (w * h * 4) as usize];
        gpu.read_pixels_rgba(w, h, &mut rgba).unwrap();

        let mut cpu_nv12 = vec![0u8; ((w * h * 3) / 2) as usize];
        convert_rgba_or_bgra_to_nv12(&rgba, &mut cpu_nv12, w, h, false);

        let mut gpu_nv12 = vec![0u8; ((w * h * 3) / 2) as usize];
        gpu.read_pixels_nv12(w, h, &mut gpu_nv12).unwrap();

        let max_diff = cpu_nv12
            .iter()
            .zip(gpu_nv12.iter())
            .map(|(&a, &b)| a.abs_diff(b))
            .max()
            .unwrap_or(0);
        assert!(
            max_diff <= 2,
            "GPU SkSL NV12 differed too much from CPU BT.601: max_diff={max_diff}"
        );
    }
}
