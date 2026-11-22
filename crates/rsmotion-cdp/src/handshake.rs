use std::time::Duration;

use rsmotion_core::{Codec, VideoConfig};
use rsmotion_encode::AudioTrackFrame;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::client::CdpClient;
use crate::error::{CdpError, Result};

/// Composition metadata returned by `window.getStaticCompositions()`, retaining schema-serialized props.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedComposition {
    pub config: VideoConfig,
    pub serialized_resolved_props: String,
    pub default_out_name: Option<String>,
    pub default_video_image_format: Option<String>,
    pub default_pixel_format: Option<String>,
    pub default_prores_profile: Option<String>,
    pub default_sample_rate: Option<u32>,
}

/// Parameters injected into every page before bundle scripts execute.
#[derive(Debug, Clone)]
pub struct InitScriptOpts {
    pub timeout_ms: u64,
    pub is_main_tab: bool,
    pub sample_rate: u32,
    pub serialized_input_props: String,
    pub initial_frame: u32,
    pub proxy_port: u16,
    pub audio_enabled: bool,
    pub video_enabled: bool,
    pub log_level: String,
}

async fn eval_js(
    client: &CdpClient,
    session_id: &str,
    expression: &str,
    await_promise: bool,
    timeout: Duration,
) -> Result<Value> {
    let res = client
        .send_session_timeout(
            session_id,
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "awaitPromise": await_promise,
                "returnByValue": true,
            }),
            timeout,
        )
        .await?;

    if let Some(exc) = res.get("exceptionDetails") {
        let desc = exc
            .pointer("/exception/description")
            .or_else(|| exc.pointer("/exception/value"))
            .or_else(|| exc.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown JS exception");

        if let Some(rest) = desc.strip_prefix("Error: __RSMOTION_TIMEOUT__:") {
            let handles = rest.lines().next().unwrap_or("").trim().to_string();
            return Err(CdpError::RenderTimeout {
                timeout_ms: timeout.as_millis() as u64,
                handles,
            });
        }
        return Err(CdpError::RenderCancelled(desc.to_string()));
    }

    Ok(res.pointer("/result/value").cloned().unwrap_or(Value::Null))
}

/// Enables `Page` and `Runtime` and installs `setPropsAndEnv` globals on the page session.
pub async fn install_init_script(
    client: &CdpClient,
    session_id: &str,
    opts: &InitScriptOpts,
) -> Result<()> {
    client
        .send_session(session_id, "Page.enable", json!({}))
        .await?;
    client
        .send_session(session_id, "Runtime.enable", json!({}))
        .await?;

    let input_props_lit = serde_json::to_string(&opts.serialized_input_props)?;
    let log_level_lit = serde_json::to_string(&opts.log_level)?;
    let script = format!(
        r#"
window.remotion_puppeteerTimeout = {timeout};
window.remotion_isMainTab = {main_tab};
window.remotion_mediaCacheSizeInBytes = null;
window.remotion_preferredEffectBackend = "auto";
window.remotion_disableWebGpuEffects = false;
window.remotion_initialMemoryAvailable = null;
window.remotion_sampleRate = {sample_rate};
if (window.process === undefined) window.process = {{}};
if (window.process.env === undefined) window.process.env = {{}};
window.process.env.NODE_ENV = "production";
window.remotion_broadcastChannel = new BroadcastChannel("remotion-video-frame-extraction");
window.remotion_inputProps = {input_props_lit};
window.remotion_initialFrame = {initial_frame};
window.remotion_attempt = 1;
window.remotion_proxyPort = {proxy_port};
window.remotion_audioEnabled = {audio_enabled};
window.remotion_videoEnabled = {video_enabled};
window.remotion_logLevel = {log_level_lit};
window.alert = (m) => {{ window.remotion_cancelledError = new Error("alert(" + (m || "") + ")").stack; }};
window.confirm = (m) => {{ window.remotion_cancelledError = new Error("confirm(" + (m || "") + ")").stack; return false; }};
"#,
        timeout = opts.timeout_ms,
        main_tab = opts.is_main_tab,
        sample_rate = opts.sample_rate,
        input_props_lit = input_props_lit,
        initial_frame = opts.initial_frame,
        proxy_port = opts.proxy_port,
        audio_enabled = opts.audio_enabled,
        video_enabled = opts.video_enabled,
        log_level_lit = log_level_lit,
    );

    client
        .send_session(
            session_id,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": script }),
        )
        .await?;

    Ok(())
}

/// Navigates to `url` and waits until Remotion's bundle APIs are registered.
pub async fn navigate_and_wait_bundle(
    client: &CdpClient,
    session_id: &str,
    url: &str,
    timeout_ms: u64,
) -> Result<()> {
    client
        .send_session(session_id, "Page.navigate", json!({ "url": url }))
        .await?;

    let expr = format!(
        r#"new Promise((resolve, reject) => {{
            const start = Date.now();
            const poll = () => {{
                if (
                    document.readyState === "complete" &&
                    typeof window.remotion_setBundleMode === "function" &&
                    window.remotion_renderReady === true
                ) {{
                    resolve(true);
                    return;
                }}
                if (Date.now() - start > {timeout_ms}) {{
                    reject(new Error("__RSMOTION_TIMEOUT__:waiting for Remotion bundle"));
                    return;
                }}
                setTimeout(poll, 5);
            }};
            poll();
        }})"#
    );
    eval_js(
        client,
        session_id,
        &expr,
        true,
        Duration::from_millis(timeout_ms + 5000),
    )
    .await?;
    Ok(())
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

/// Evaluates `window.getStaticCompositions()` in evaluation bundle mode.
pub async fn get_compositions(
    client: &CdpClient,
    session_id: &str,
    timeout_ms: u64,
) -> Result<Vec<ResolvedComposition>> {
    let expr = format!(
        r#"(async () => {{
            const waitReady = () => new Promise((resolve, reject) => {{
                const start = Date.now();
                let ticks = 0;
                const check = () => {{
                    if (window.remotion_cancelledError !== undefined) {{
                        reject(new Error(String(window.remotion_cancelledError)));
                        return;
                    }}
                    if (window.remotion_renderReady === true) {{
                        resolve();
                        return;
                    }}
                    if (Date.now() - start > {timeout_ms}) {{
                        const handles = Object.keys(window.remotion_delayRenderTimeouts || {{}})
                            .map((id, i) => `${{i + 1}}. ${{(window.remotion_delayRenderTimeouts[id] && window.remotion_delayRenderTimeouts[id].label) || "(no label)"}}`)
                            .join(", ");
                        reject(new Error("__RSMOTION_TIMEOUT__:" + handles));
                        return;
                    }}
                    ticks += 1;
                    setTimeout(check, ticks < 5 ? 0 : 2);
                }};
                check();
            }});
            window.remotion_setBundleMode({{ type: "evaluation" }});
            await waitReady();
            const compStart = Date.now();
            while (true) {{
                try {{
                    return await window.getStaticCompositions();
                }} catch (err) {{
                    const msg = err && err.message ? String(err.message) : String(err);
                    if (msg.includes("Unexpectedly did not have a CompositionManager") && Date.now() - compStart < {timeout_ms}) {{
                        await new Promise((r) => setTimeout(r, 5));
                        continue;
                    }}
                    throw err;
                }}
            }}
        }})()"#
    );

    let val = eval_js(
        client,
        session_id,
        &expr,
        true,
        Duration::from_millis(timeout_ms + 5000),
    )
    .await?;

    let arr = val.as_array().ok_or_else(|| {
        CdpError::Protocol("getStaticCompositions did not return an array".into())
    })?;

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

        let props: Value = serde_json::from_str(&serialized_resolved_props).unwrap_or(json!({}));
        let default_props: Value =
            serde_json::from_str(&serialized_default_props).unwrap_or(json!({}));

        let default_codec_str = item
            .get("defaultCodec")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        out.push(ResolvedComposition {
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

/// Sets viewport metrics and mounts the target composition on a page session.
pub async fn mount_composition(
    client: &CdpClient,
    session_id: &str,
    comp: &ResolvedComposition,
    scale: f64,
    timeout_ms: u64,
) -> Result<()> {
    client
        .send_session(
            session_id,
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": comp.config.width,
                "height": comp.config.height,
                "deviceScaleFactor": scale,
                "mobile": false,
            }),
        )
        .await?;

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

    let expr = format!(
        r#"(async () => {{
            const waitReady = () => new Promise((resolve, reject) => {{
                const start = Date.now();
                let ticks = 0;
                const check = () => {{
                    if (window.remotion_cancelledError !== undefined) {{
                        reject(new Error(String(window.remotion_cancelledError)));
                        return;
                    }}
                    if (
                        window.remotion_renderReady === true &&
                        typeof window.remotion_setFrame === "function" &&
                        typeof window.remotion_setMediaEnabled === "function"
                    ) {{
                        resolve();
                        return;
                    }}
                    if (Date.now() - start > {timeout_ms}) {{
                        const handles = Object.keys(window.remotion_delayRenderTimeouts || {{}})
                            .map((id, i) => `${{i + 1}}. ${{(window.remotion_delayRenderTimeouts[id] && window.remotion_delayRenderTimeouts[id].label) || "(no label)"}}`)
                            .join(", ");
                        reject(new Error("__RSMOTION_TIMEOUT__:" + handles));
                        return;
                    }}
                    ticks += 1;
                    setTimeout(check, ticks < 5 ? 0 : 2);
                }};
                check();
            }});
            window.remotion_setBundleMode({payload_lit});
            await waitReady();
            return true;
        }})()"#
    );

    eval_js(
        client,
        session_id,
        &expr,
        true,
        Duration::from_millis(timeout_ms + 5000),
    )
    .await?;
    Ok(())
}

/// Seeks the page to `frame`, waits for `remotion_renderReady` + `document.fonts.ready`,
/// and returns collected audio/video assets in a single CDP roundtrip.
pub async fn seek_and_collect(
    client: &CdpClient,
    session_id: &str,
    composition_id: &str,
    frame: u32,
    video_enabled: bool,
    audio_enabled: bool,
    timeout_ms: u64,
) -> Result<Vec<AudioTrackFrame>> {
    let comp_lit = serde_json::to_string(composition_id)?;
    let expr = format!(
        r#"(async () => {{
            const waitReady = () => new Promise((resolve, reject) => {{
                const start = Date.now();
                let ticks = 0;
                const check = () => {{
                    if (window.remotion_cancelledError !== undefined) {{
                        reject(new Error(String(window.remotion_cancelledError)));
                        return;
                    }}
                    if (
                        window.remotion_renderReady === true &&
                        typeof window.remotion_setFrame === "function" &&
                        typeof window.remotion_setMediaEnabled === "function"
                    ) {{
                        resolve();
                        return;
                    }}
                    if (Date.now() - start > {timeout_ms}) {{
                        const handles = Object.keys(window.remotion_delayRenderTimeouts || {{}})
                            .map((id, i) => `${{i + 1}}. ${{(window.remotion_delayRenderTimeouts[id] && window.remotion_delayRenderTimeouts[id].label) || "(no label)"}}`)
                            .join(", ");
                        reject(new Error("__RSMOTION_TIMEOUT__:" + handles));
                        return;
                    }}
                    ticks += 1;
                    setTimeout(check, ticks < 5 ? 0 : 2);
                }};
                check();
            }});
            await waitReady();
            window.remotion_setMediaEnabled({video_enabled}, {audio_enabled});
            window.remotion_setFrame({frame}, {comp_lit}, 1);
            await waitReady();
            if ({video_enabled} && document.fonts && document.fonts.ready) {{
                await document.fonts.ready;
            }}
            const raw = typeof window.remotion_collectAssets === "function" ? window.remotion_collectAssets() : [];
            return raw.filter((a) => a && (a.type === "audio" || a.type === "video")).map((a) => ({{
                id: String(a.id || ""),
                src: String(a.src || ""),
                frame: Number(a.frame || 0),
                volume: Number(a.volume ?? 1),
                media_frame: Number(a.mediaFrame ?? 0),
                playback_rate: Number(a.playbackRate ?? 1),
                audio_start_frame: Number(a.audioStartFrame ?? 0),
            }}));
        }})()"#
    );

    let val = eval_js(
        client,
        session_id,
        &expr,
        true,
        Duration::from_millis(timeout_ms + 5000),
    )
    .await?;

    let tracks: Vec<AudioTrackFrame> = serde_json::from_value(val)?;
    Ok(tracks)
}
