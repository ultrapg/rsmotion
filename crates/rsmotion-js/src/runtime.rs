use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use deno_core::{v8, JsRuntime, RuntimeOptions};
use rsmotion_core::{Codec, VideoConfig};
use rsmotion_scene::{SceneTree, Tier1Composition};
use serde_json::{json, Value};

use crate::error::{JsHostError, Result};
use crate::ops::{rsmotion_host, HostState};

const HOST_JS: &str = include_str!("host.js");

#[derive(Debug, Clone)]
enum HtmlScript {
    Inline(String),
    External(PathBuf),
}

fn extract_attr(attrs_raw: &str, name: &str) -> Option<String> {
    let lower = attrs_raw.to_ascii_lowercase();
    let needle = format!("{name}=");
    let pos = lower.find(&needle)?;
    let rest = attrs_raw[pos + needle.len()..].trim_start();
    if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let after = &rest[1..];
        let end = after.find(q)?;
        Some(after[..end].to_string())
    } else {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

fn parse_html_scripts(bundle_dir: &Path, html: &str) -> Result<Vec<HtmlScript>> {
    let lower = html.to_ascii_lowercase();
    let mut scripts = Vec::new();
    let mut cursor = 0usize;

    while let Some(rel_start) = lower[cursor..].find("<script") {
        let tag_start = cursor + rel_start;
        let after_tag = tag_start + "<script".len();
        let Some(rel_gt) = lower[after_tag..].find('>') else {
            return Err(JsHostError::BundleParse(
                "unterminated <script> tag in index.html".to_string(),
            ));
        };
        let tag_close = after_tag + rel_gt;
        let attrs = &html[after_tag..tag_close];
        let content_start = tag_close + 1;

        let Some(rel_end) = lower[content_start..].find("</script>") else {
            return Err(JsHostError::BundleParse(
                "missing </script> in index.html".to_string(),
            ));
        };
        let content_end = content_start + rel_end;

        if let Some(src) = extract_attr(attrs, "src") {
            let clean = src.split(['?', '#']).next().unwrap_or(&src);
            let clean = clean
                .strip_prefix("./")
                .unwrap_or(clean)
                .trim_start_matches('/');
            let path = bundle_dir.join(clean);
            scripts.push(HtmlScript::External(path));
        } else {
            let inline = html[content_start..content_end].trim();
            if !inline.is_empty() {
                scripts.push(HtmlScript::Inline(inline.to_string()));
            }
        }

        cursor = content_end + "</script>".len();
    }

    if scripts.is_empty() {
        return Err(JsHostError::BundleParse(format!(
            "no <script> tags found in {}",
            bundle_dir.join("index.html").display()
        )));
    }
    Ok(scripts)
}

#[derive(Debug, Clone)]
enum HtmlStyleEntry {
    Inline(String),
    Link(String),
}

fn parse_html_styles(html: &str) -> Vec<HtmlStyleEntry> {
    let lower = html.to_ascii_lowercase();
    let mut entries = Vec::new();
    let mut cursor = 0usize;

    while cursor < lower.len() {
        let next_style = lower[cursor..].find("<style").map(|r| cursor + r);
        let next_link = lower[cursor..].find("<link").map(|r| cursor + r);
        let take_style = match (next_style, next_link) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(s), Some(l)) => s < l,
        };
        if take_style {
            let s = next_style.unwrap();
            let after = s + "<style".len();
            let Some(rel_gt) = lower[after..].find('>') else {
                break;
            };
            let content_start = after + rel_gt + 1;
            let Some(rel_end) = lower[content_start..].find("</style>") else {
                cursor = content_start;
                continue;
            };
            let content_end = content_start + rel_end;
            let css = html[content_start..content_end].trim();
            if !css.is_empty() {
                entries.push(HtmlStyleEntry::Inline(css.to_string()));
            }
            cursor = content_end + "</style>".len();
        } else {
            let l = next_link.unwrap();
            let after = l + "<link".len();
            let Some(rel_gt) = lower[after..].find('>') else {
                break;
            };
            let attrs = &html[after..after + rel_gt];
            if extract_attr(attrs, "rel").is_some_and(|r| r.eq_ignore_ascii_case("stylesheet")) {
                if let Some(href) = extract_attr(attrs, "href") {
                    entries.push(HtmlStyleEntry::Link(href));
                }
            }
            cursor = after + rel_gt + 1;
        }
    }
    entries
}

fn parse_codec_opt(s: Option<&str>) -> Option<Codec> {
    match s? {
        "h264" => Some(Codec::H264),
        "h265" => Some(Codec::H265),
        "vp8" => Some(Codec::Vp8),
        "vp9" => Some(Codec::Vp9),
        "prores" => Some(Codec::ProRes),
        "gif" => Some(Codec::Gif),
        "mp3" => Some(Codec::Mp3),
        "aac" => Some(Codec::Aac),
        "wav" => Some(Codec::Wav),
        _ => None,
    }
}

fn patch_lottie_bundle(code: &str) -> Cow<'_, str> {
    if !code.contains(".setSubframeRendering") || !code.contains(".loadAnimation") {
        return Cow::Borrowed(code);
    }
    let bytes = code.as_bytes();
    for (sub_pos, _) in code.match_indices(".setSubframeRendering") {
        let win_start = sub_pos.saturating_sub(300);
        let window = &code[win_start..sub_pos];
        let Some(rel_la) = window.rfind(".loadAnimation") else {
            continue;
        };
        let mut idx = win_start + rel_la + ".loadAnimation".len();
        while idx < sub_pos && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= sub_pos || bytes[idx] != b'=' || bytes.get(idx + 1) == Some(&b'=') {
            continue;
        }
        idx += 1;
        while idx < sub_pos && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        let rhs_start = idx;
        let mut depth_paren = 0usize;
        let mut depth_brace = 0usize;
        let mut depth_bracket = 0usize;
        while idx < sub_pos {
            match bytes[idx] {
                b'(' => depth_paren += 1,
                b')' => depth_paren = depth_paren.saturating_sub(1),
                b'{' => depth_brace += 1,
                b'}' => depth_brace = depth_brace.saturating_sub(1),
                b'[' => depth_bracket += 1,
                b']' => depth_bracket = depth_bracket.saturating_sub(1),
                b',' | b';' | b'\n' | b'\r'
                    if depth_paren == 0 && depth_brace == 0 && depth_bracket == 0 =>
                {
                    break;
                }
                _ => {}
            }
            idx += 1;
        }
        let rhs = code[rhs_start..idx].trim();
        if rhs.is_empty() || rhs.contains("__rsmotion_wrap_lottie") {
            continue;
        }
        let wrapped = format!(
            "(globalThis.__rsmotion_wrap_lottie?globalThis.__rsmotion_wrap_lottie({rhs}):({rhs}))"
        );
        let mut out = String::with_capacity(code.len() + wrapped.len());
        out.push_str(&code[..rhs_start]);
        out.push_str(&wrapped);
        out.push_str(&code[idx..]);
        return Cow::Owned(out);
    }
    Cow::Borrowed(code)
}

/// Embedded V8 host capable of running unmodified Remotion Webpack bundles headless.
pub struct V8BundleHost {
    rt: JsRuntime,
    timeout_ms: u64,
    // Drop AFTER `rt` so V8's captured tokio Handle stays valid through isolate teardown.
    _fallback_tokio: Option<tokio::runtime::Runtime>,
}

impl V8BundleHost {
    /// Boots V8, installs the Virtual DOM host, and executes `bundle_dir/index.html` + `bundle.js`.
    pub fn new(bundle_dir: &Path, input_props: &Value, timeout_ms: u64) -> Result<Self> {
        Self::new_with_options(bundle_dir, input_props, timeout_ms, false)
    }

    /// Boots V8 with explicit offline cache policy (`offline = true` blocks missing network fetches).
    pub fn new_with_options(
        bundle_dir: &Path,
        input_props: &Value,
        timeout_ms: u64,
        offline: bool,
    ) -> Result<Self> {
        let index_path = bundle_dir.join("index.html");
        if !index_path.is_file() {
            return Err(JsHostError::BundleParse(format!(
                "missing index.html at {}",
                index_path.display()
            )));
        }
        let html = std::fs::read_to_string(&index_path)?;
        let scripts = parse_html_scripts(bundle_dir, &html)?;
        let html_styles = parse_html_styles(&html);

        let fallback_tokio = if tokio::runtime::Handle::try_current().is_err() {
            Some(
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?,
            )
        } else {
            None
        };
        let _guard = fallback_tokio.as_ref().map(|rt| rt.enter());

        let mut rt = JsRuntime::new(RuntimeOptions {
            extensions: vec![rsmotion_host::init(bundle_dir.to_path_buf(), offline)],
            ..Default::default()
        });

        rt.execute_script("<rsmotion:host.js>", HOST_JS)
            .map_err(|e| JsHostError::V8(e.to_string()))?;

        if !html_styles.is_empty() {
            let mut inject = String::from("(() => {\n");
            for entry in html_styles {
                match entry {
                    HtmlStyleEntry::Inline(css) => {
                        let lit = serde_json::to_string(&css)?;
                        inject.push_str(&format!(
                            "  const s = document.createElement('style'); s.textContent = {lit}; document.head.appendChild(s);\n"
                        ));
                    }
                    HtmlStyleEntry::Link(href) => {
                        let lit = serde_json::to_string(&href)?;
                        inject.push_str(&format!(
                            "  const l = document.createElement('link'); l.setAttribute('rel', 'stylesheet'); l.setAttribute('href', {lit}); document.head.appendChild(l);\n"
                        ));
                    }
                }
            }
            inject.push_str("})();\n");
            rt.execute_script("<rsmotion:html-styles.js>", inject)
                .map_err(|e| JsHostError::V8(e.to_string()))?;
        }

        let serialized_props = serde_json::to_string(input_props)?;
        let props_lit = serde_json::to_string(&serialized_props)?;
        let env_js = format!(
            r#"
window.remotion_puppeteerTimeout = {timeout_ms};
window.remotion_isMainTab = true;
window.remotion_mediaCacheSizeInBytes = null;
window.remotion_preferredEffectBackend = "auto";
window.remotion_disableWebGpuEffects = false;
window.remotion_initialMemoryAvailable = null;
window.remotion_sampleRate = 48000;
if (window.process === undefined) window.process = {{}};
if (window.process.env === undefined) window.process.env = {{}};
window.process.env.NODE_ENV = "production";
window.remotion_broadcastChannel = new BroadcastChannel("remotion-video-frame-extraction");
window.remotion_inputProps = {props_lit};
window.remotion_initialFrame = 0;
window.remotion_attempt = 1;
window.remotion_proxyPort = 1;
window.remotion_audioEnabled = true;
window.remotion_videoEnabled = true;
window.remotion_logLevel = "warn";
window.alert = (m) => {{ window.remotion_cancelledError = new Error("alert(" + (m || "") + ")").stack; }};
window.confirm = (m) => {{ window.remotion_cancelledError = new Error("confirm(" + (m || "") + ")").stack; return false; }};
"#
        );
        rt.execute_script("<rsmotion:set-props-and-env.js>", env_js)
            .map_err(|e| JsHostError::V8(e.to_string()))?;

        for (idx, script) in scripts.into_iter().enumerate() {
            match script {
                HtmlScript::Inline(code) => {
                    let name = format!("<index.html:inline:{idx}>");
                    rt.execute_script(name, code)
                        .map_err(|e| JsHostError::V8(e.to_string()))?;
                }
                HtmlScript::External(path) => {
                    let raw_code = std::fs::read_to_string(&path)?;
                    let patched = patch_lottie_bundle(&raw_code).into_owned();
                    let name = path.display().to_string();
                    rt.execute_script(name, patched)
                        .map_err(|e| JsHostError::V8(e.to_string()))?;
                }
            }
        }

        drop(_guard);
        let mut host = Self {
            rt,
            timeout_ms,
            _fallback_tokio: fallback_tokio,
        };
        host.pump_until(
            "typeof window.remotion_setBundleMode === 'function' && window.remotion_renderReady === true",
            "booting Remotion bundle",
        )?;
        Ok(host)
    }

    fn eval_bool(&mut self, expr: &str) -> Result<bool> {
        let global = self
            .rt
            .execute_script("<rsmotion:check>", expr.to_string())
            .map_err(|e| JsHostError::V8(e.to_string()))?;
        deno_core::scope!(scope, &mut self.rt);
        let local = v8::Local::new(scope, global);
        Ok(local.boolean_value(scope))
    }

    fn eval_string(&mut self, expr: &str) -> Result<String> {
        let global = self
            .rt
            .execute_script("<rsmotion:eval>", expr.to_string())
            .map_err(|e| JsHostError::V8(e.to_string()))?;
        deno_core::scope!(scope, &mut self.rt);
        let local = v8::Local::new(scope, global);
        if local.is_null_or_undefined() {
            return Ok(String::new());
        }
        Ok(local.to_rust_string_lossy(scope))
    }

    fn perform_microtasks(&mut self) {
        deno_core::scope!(scope, &mut self.rt);
        scope.perform_microtask_checkpoint();
    }

    fn check_cancelled(&mut self) -> Result<()> {
        let err = self.eval_string(
            "window.remotion_cancelledError !== undefined ? String(window.remotion_cancelledError) : ''",
        )?;
        if !err.is_empty() {
            return Err(JsHostError::RenderCancelled(err));
        }
        Ok(())
    }

    fn pending_handles_desc(&mut self) -> String {
        self.eval_string(
            r#"Object.keys(window.remotion_delayRenderTimeouts || {})
                .map((id, i) => `${i + 1}. ${(window.remotion_delayRenderTimeouts[id] && window.remotion_delayRenderTimeouts[id].label) || "(no label)"}`)
                .join(", ")"#,
        )
        .unwrap_or_default()
    }

    fn pump_until(&mut self, condition_expr: &str, context_label: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_millis(self.timeout_ms);
        loop {
            self.perform_microtasks();
            self.check_cancelled()?;
            if self.eval_bool(condition_expr)? {
                return Ok(());
            }

            let did_work = self.eval_bool("window.__rsmotion_drain_tick()")?;
            self.perform_microtasks();
            self.check_cancelled()?;
            if self.eval_bool(condition_expr)? {
                return Ok(());
            }

            if Instant::now() >= deadline {
                let handles = self.pending_handles_desc();
                let detail = if handles.is_empty() {
                    context_label.to_string()
                } else {
                    format!("{context_label}: {handles}")
                };
                return Err(JsHostError::RenderTimeout {
                    timeout_ms: self.timeout_ms,
                    handles: detail,
                });
            }

            if !did_work {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// Evaluates `window.getStaticCompositions()` in evaluation bundle mode.
    pub fn get_compositions(&mut self) -> Result<Vec<Tier1Composition>> {
        self.rt
            .execute_script(
                "<rsmotion:eval-mode>",
                r#"window.remotion_setBundleMode({ type: "evaluation" });"#,
            )
            .map_err(|e| JsHostError::V8(e.to_string()))?;

        self.pump_until(
            "window.remotion_renderReady === true",
            "switching to evaluation mode",
        )?;

        let eval_script = format!(
            r#"
window.__rsmotion_eval_state = {{ done: false, resultJson: "", error: "" }};
(async () => {{
  const deadline = Date.now() + {timeout_ms};
  try {{
    while (true) {{
      try {{
        const list = await window.getStaticCompositions();
        window.__rsmotion_eval_state = {{
          done: true,
          resultJson: JSON.stringify(list),
          error: "",
        }};
        return;
      }} catch (err) {{
        const msg = err && err.message ? String(err.message) : String(err);
        if (msg.includes("Unexpectedly did not have a CompositionManager") && Date.now() < deadline) {{
          await new Promise((r) => setTimeout(r, 1));
          continue;
        }}
        throw err;
      }}
    }}
  }} catch (err) {{
    window.__rsmotion_eval_state = {{
      done: true,
      resultJson: "",
      error: String(err && (err.stack || err.message || err)),
    }};
  }}
}})();
"#,
            timeout_ms = self.timeout_ms
        );

        self.rt
            .execute_script("<rsmotion:get-static-compositions>", eval_script)
            .map_err(|e| JsHostError::V8(e.to_string()))?;

        self.pump_until(
            "window.__rsmotion_eval_state.done === true && (window.__rsmotion_eval_state.error !== '' || window.remotion_renderReady === true)",
            "evaluating getStaticCompositions()",
        )?;

        let err = self.eval_string("window.__rsmotion_eval_state.error")?;
        if !err.is_empty() {
            return Err(JsHostError::RenderCancelled(err));
        }

        let raw_json = self.eval_string("window.__rsmotion_eval_state.resultJson")?;
        let arr: Vec<Value> = serde_json::from_str(&raw_json)?;

        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let width = item.get("width").and_then(|v| v.as_u64()).unwrap_or(1920) as u32;
            let height = item.get("height").and_then(|v| v.as_u64()).unwrap_or(1080) as u32;
            let fps = item.get("fps").and_then(|v| v.as_f64()).unwrap_or(30.0);
            let duration_in_frames = item
                .get("durationInFrames")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as u32;

            let serialized_resolved_props = item
                .get("serializedResolvedPropsWithCustomSchema")
                .and_then(|v| v.as_str())
                .unwrap_or("{}")
                .to_string();
            let serialized_default_props = item
                .get("serializedDefaultPropsWithCustomSchema")
                .and_then(|v| v.as_str())
                .unwrap_or("{}")
                .to_string();

            let props: Value =
                serde_json::from_str(&serialized_resolved_props).unwrap_or(json!({}));
            let default_props: Value =
                serde_json::from_str(&serialized_default_props).unwrap_or(json!({}));

            let default_codec_str = item
                .get("defaultCodec")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            out.push(Tier1Composition {
                config: VideoConfig {
                    id,
                    width,
                    height,
                    fps,
                    duration_in_frames,
                    props,
                    default_props,
                    default_codec: parse_codec_opt(default_codec_str.as_deref()),
                },
                serialized_resolved_props,
                default_out_name: item
                    .get("defaultOutName")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                default_video_image_format: item
                    .get("defaultVideoImageFormat")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                default_pixel_format: item
                    .get("defaultPixelFormat")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                default_prores_profile: item
                    .get("defaultProResProfile")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                default_sample_rate: item
                    .get("defaultSampleRate")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32),
            });
        }
        Ok(out)
    }

    /// Mounts `comp` as the active composition in `renderEntry.tsx`.
    pub fn mount_composition(&mut self, comp: &Tier1Composition) -> Result<()> {
        let payload = json!({
            "type": "composition",
            "compositionName": comp.config.id,
            "serializedResolvedPropsWithSchema": comp.serialized_resolved_props,
            "compositionDurationInFrames": comp.config.duration_in_frames,
            "compositionFps": comp.config.fps,
            "compositionHeight": comp.config.height,
            "compositionWidth": comp.config.width,
            "compositionDefaultCodec": comp.config.default_codec.map(|c| c.as_str()).unwrap_or("h264"),
            "compositionDefaultOutName": comp.default_out_name,
            "compositionDefaultVideoImageFormat": comp.default_video_image_format,
            "compositionDefaultPixelFormat": comp.default_pixel_format,
            "compositionDefaultProResProfile": comp.default_prores_profile,
            "compositionDefaultSampleRate": comp.default_sample_rate,
        });
        let payload_lit = serde_json::to_string(&payload)?;
        let script = format!(
            "window.innerWidth = {w}; window.innerHeight = {h}; window.remotion_setBundleMode({payload_lit});",
            w = comp.config.width,
            h = comp.config.height,
        );
        self.rt
            .execute_script("<rsmotion:mount-composition>", script)
            .map_err(|e| JsHostError::V8(e.to_string()))?;

        self.pump_until(
            "window.remotion_renderReady === true && typeof window.remotion_setFrame === 'function' && typeof window.remotion_setMediaEnabled === 'function'",
            "mounting composition",
        )?;
        Ok(())
    }

    /// Seeks to `frame`, settles React + `delayRender`, and returns a [`SceneTree`] snapshot.
    pub fn render_frame_tree(
        &mut self,
        comp_id: &str,
        width: u32,
        height: u32,
        frame: u32,
    ) -> Result<SceneTree> {
        let comp_lit = serde_json::to_string(comp_id)?;
        let seek_js = format!(
            "window.remotion_setMediaEnabled(true, true); window.remotion_setFrame({frame}, {comp_lit}, 1);"
        );
        self.rt
            .execute_script("<rsmotion:set-frame>", seek_js)
            .map_err(|e| JsHostError::V8(e.to_string()))?;

        self.pump_until(
            "window.remotion_renderReady === true && !window.__rsmotion_has_pending_short_work()",
            &format!("rendering frame {frame}"),
        )?;

        let snap_js =
            format!("JSON.stringify(window.__rsmotion_snapshot({width}, {height}, {frame}))");
        let snap_json = self.eval_string(&snap_js)?;
        let mut tree: SceneTree = serde_json::from_str(&snap_json)?;

        let op_state = self.rt.op_state();
        let state = op_state.borrow();
        let host = state.borrow::<HostState>();
        tree.fonts = host.fonts.clone();
        tree.lottie_assets = host.lotties.clone();

        Ok(tree)
    }
}
