use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use rolldown::plugin::{
    HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput,
    HookResolveIdReturn, HookUsage, Plugin, PluginContext, PluginHookMeta, PluginOrder,
    SharedLoadPluginContext,
};
use rolldown::{
    Bundler, BundlerBuilder, BundlerOptions, BundlerTransformOptions, CodeSplittingMode, Either,
    InputItem, JsxOptions, ModuleType, OutputFormat, Platform, ResolveOptions, TsConfig,
};
use rolldown_utils::indexmap::FxIndexMap;
use sha2::{Digest, Sha256};

use crate::tailwind;

const VIRTUAL_ENTRY_FILENAME: &str = "__rsmotion_virtual_entry__.mjs";
const SETUP_ENV_ID: &str = "\0rsmotion-setup-env.mjs";
const SETUP_ENV_SOURCE: &str = r#"(() => {
  if (typeof window === "undefined") return;
  const proc = (window.process ??= {});
  const env = (proc.env ??= {});
  if (typeof window.remotion_envVariables === "string" && window.remotion_envVariables.length > 0) {
    try {
      Object.assign(env, JSON.parse(window.remotion_envVariables));
    } catch {}
  }
  env.NODE_ENV = "production";
  window.remotion_enableSequenceStackTraces ??= () => {};
})();
"#;

const ASSET_EXTENSIONS: &[&str] = &[
    "png", "svg", "jpg", "jpeg", "jxl", "webp", "gif", "bmp", "ico", "avif", "webm", "mp4", "mov",
    "mp3", "m4a", "wav", "aac", "ogg", "woff", "woff2", "otf", "ttf", "eot",
];

fn clean_specifier_path(id: &str) -> &str {
    id.split(['?', '#']).next().unwrap_or(id)
}

fn has_asset_extension(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    matches!(ext.as_deref(), Some(e) if ASSET_EXTENSIONS.contains(&e))
}

fn find_package_file(start: &Path, rel: &str) -> Option<String> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        let candidate = dir.join("node_modules").join(rel);
        if candidate.is_file() {
            return Some(
                fs::canonicalize(&candidate)
                    .unwrap_or(candidate)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        cur = dir.parent();
    }
    None
}

fn collect_singleton_aliases(remotion_root: &Path) -> HashMap<&'static str, String> {
    let candidates: &[(&str, &[&str])] = &[
        (
            "remotion",
            &["remotion/dist/esm/index.mjs", "remotion/index.js"],
        ),
        (
            "remotion/no-react",
            &["remotion/dist/esm/no-react.mjs", "remotion/no-react.js"],
        ),
        (
            "remotion/version",
            &["remotion/dist/esm/version.mjs", "remotion/version.js"],
        ),
        ("react", &["react/index.js"]),
        ("react/jsx-runtime", &["react/jsx-runtime.js"]),
        ("react/jsx-dev-runtime", &["react/jsx-dev-runtime.js"]),
        ("react-dom", &["react-dom/index.js"]),
        (
            "react-dom/client",
            &["react-dom/client.js", "react-dom/index.js"],
        ),
    ];

    let mut map = HashMap::new();
    for (spec, paths) in candidates {
        for rel in *paths {
            if let Some(found) = find_package_file(remotion_root, rel) {
                map.insert(*spec, found);
                break;
            }
        }
    }
    map
}

fn build_virtual_entry_source(user_entry: &Path) -> Result<String> {
    let entry_literal = serde_json::to_string(&user_entry.to_string_lossy().into_owned())?;
    let setup_literal = serde_json::to_string(SETUP_ENV_ID)?;
    Ok(format!(
        r#"// Auto-generated headless mount harness for rsmotion-bundler
import {setup_literal};
import {{ useContext, useEffect, useLayoutEffect, useRef, useState }} from "react";
import {{ createRoot }} from "react-dom/client";
import {{ jsx, jsxs }} from "react/jsx-runtime";
import {{
  Internals,
  continueRender,
  delayRender,
  getInputProps,
  getRemotionEnvironment,
  useDelayRender,
}} from "remotion";
import {{ NoReactInternals }} from "remotion/no-react";

import {entry_literal};

let activeMode = {{ type: "index" }};
let mountRoot = null;

const rootContextProps = (videoEnabled, audioEnabled) => ({{
  frameState: null,
  audioEnabled,
  videoEnabled,
  logLevel: (typeof window !== "undefined" && window.remotion_logLevel) || "info",
  numberOfAudioTags: 0,
  audioLatencyHint:
    (typeof window !== "undefined" && window.remotion_audioLatencyHint) || "playback",
  previewSampleRate:
    typeof window !== "undefined" ? (window.remotion_sampleRate ?? 48000) : 48000,
  _experimentalKeepAudioContextAlive: false,
}});

const MediaSwitchProvider = ({{ children }}) => {{
  const [media, setMedia] = useState(() => ({{
    videoEnabled:
      typeof window !== "undefined" ? (window.remotion_videoEnabled ?? true) : true,
    audioEnabled:
      typeof window !== "undefined" ? (window.remotion_audioEnabled ?? true) : true,
  }}));
  const mediaRef = useRef(media);
  mediaRef.current = media;

  useLayoutEffect(() => {{
    window.remotion_setMediaEnabled = (videoEnabled, audioEnabled) => {{
      window.remotion_videoEnabled = videoEnabled;
      window.remotion_audioEnabled = audioEnabled;
      const h = delayRender("rsmotion: updating media flags");
      if (
        mediaRef.current.videoEnabled === videoEnabled &&
        mediaRef.current.audioEnabled === audioEnabled
      ) {{
        continueRender(h);
        return;
      }}
      setMedia({{ videoEnabled, audioEnabled }});
      requestAnimationFrame(() => continueRender(h));
    }};
  }}, []);

  return jsx(Internals.RemotionRootContexts, {{
    ...rootContextProps(media.videoEnabled, media.audioEnabled),
    children,
  }});
}};

const CompositionStage = ({{ compositionId }}) => {{
  const {{ compositions, currentCompositionMetadata, canvasContent }} = useContext(
    Internals.CompositionManager
  );
  const {{ setCanvasContent }} = useContext(Internals.CompositionSetters);
  const hostRef = useRef(null);
  const {{ delayRender: holdRender, continueRender: releaseRender }} = useDelayRender();
  const handleRef = useRef(null);
  const mountedRef = useRef(false);

  useLayoutEffect(() => {{
    if (mountedRef.current) {{
      return;
    }}
    const h = holdRender(`rsmotion: awaiting stage "${{compositionId}}"`);
    handleRef.current = h;
    return () => {{
      releaseRender(h);
      handleRef.current = null;
    }};
  }}, [holdRender, releaseRender, compositionId]);

  useEffect(() => {{
    if (compositions.length === 0) {{
      return;
    }}
    const match = compositions.find((c) => c.id === compositionId);
    if (!match) {{
      const avail = compositions.map((c) => c.id).join(", ");
      throw new Error(
        `rsmotion: composition "${{compositionId}}" not found (available: ${{avail}})`
      );
    }}
    setCanvasContent({{ type: "composition", compositionId: match.id }});
  }}, [compositions, compositionId, currentCompositionMetadata, setCanvasContent]);

  useEffect(() => {{
    if (!canvasContent) {{
      return;
    }}
    const host = hostRef.current;
    if (!host) {{
      throw new Error("rsmotion: composition host container did not mount");
    }}
    const portal = Internals.portalNode();
    host.appendChild(portal);
    mountedRef.current = true;
    if (handleRef.current !== null) {{
      releaseRender(handleRef.current);
      handleRef.current = null;
    }}
    return () => {{
      if (host.contains(portal)) {{
        host.removeChild(portal);
      }}
    }};
  }}, [canvasContent, releaseRender]);

  if (!currentCompositionMetadata) {{
    return null;
  }}

  return jsx("div", {{
    ref: hostRef,
    id: "remotion-canvas",
    style: {{
      width: currentCompositionMetadata.width,
      height: currentCompositionMetadata.height,
      display: "flex",
      backgroundColor: "transparent",
    }},
  }});
}};

const mount = (node) => {{
  if (!mountRoot) {{
    const container = document.getElementById("video-container");
    if (!container) {{
      throw new Error("rsmotion: #video-container element is missing");
    }}
    mountRoot = createRoot(container);
  }}
  mountRoot.render(node);
}};

const renderMode = (Root, mode) => {{
  if (mode.type === "composition") {{
    const metadata = {{
      props: NoReactInternals.deserializeJSONWithSpecialTypes(
        mode.serializedResolvedPropsWithSchema
      ),
      durationInFrames: mode.compositionDurationInFrames,
      fps: mode.compositionFps,
      height: mode.compositionHeight,
      width: mode.compositionWidth,
      defaultCodec: mode.compositionDefaultCodec,
      defaultOutName: mode.compositionDefaultOutName,
      defaultVideoImageFormat: mode.compositionDefaultVideoImageFormat,
      defaultPixelFormat: mode.compositionDefaultPixelFormat,
      defaultProResProfile: mode.compositionDefaultProResProfile,
      defaultSampleRate: mode.compositionDefaultSampleRate,
    }};
    mount(
      jsx(Internals.CompositionManagerProvider, {{
        initialCanvasContent: null,
        onlyRenderComposition: mode.compositionName,
        currentCompositionMetadata: metadata,
        initialCompositions: [],
        children: jsx(MediaSwitchProvider, {{
          children: jsxs(Internals.RenderAssetManagerProvider, {{
            collectAssets: null,
            children: [
              jsx(Root, {{}}),
              jsx(CompositionStage, {{ compositionId: mode.compositionName }}),
            ],
          }}),
        }}),
      }})
    );
    return;
  }}

  if (mode.type === "evaluation") {{
    getInputProps();
    const videoOn = typeof window !== "undefined" ? (window.remotion_videoEnabled ?? true) : true;
    const audioOn = typeof window !== "undefined" ? (window.remotion_audioEnabled ?? true) : true;
    mount(
      jsx(Internals.CompositionManagerProvider, {{
        initialCanvasContent: null,
        onlyRenderComposition: null,
        currentCompositionMetadata: null,
        initialCompositions: [],
        children: jsx(Internals.RemotionRootContexts, {{
          ...rootContextProps(videoOn, audioOn),
          children: jsx(Internals.RenderAssetManagerProvider, {{
            collectAssets: null,
            children: jsx(Root, {{}}),
          }}),
        }}),
      }})
    );
  }}
}};

const bootHandle = delayRender("rsmotion: waiting for registered root", {{
  timeoutInMilliseconds:
    (typeof window !== "undefined" && window.remotion_puppeteerTimeout) || 10000,
}});

Internals.waitForRoot((Root) => {{
  renderMode(Root, activeMode);
  continueRender(bootHandle);
}});

if (typeof window !== "undefined") {{
  const getRegisteredCompositions = () => {{
    if (!Internals.getRoot()) {{
      throw new Error("rsmotion: root component was not registered with registerRoot()");
    }}
    const mgr = Internals.compositionsRef && Internals.compositionsRef.current;
    if (!mgr) {{
      throw new Error("rsmotion: CompositionManager is unavailable");
    }}
    return mgr.getCompositions();
  }};

  const resolveComposition = async (comp) => {{
    const h = delayRender(`rsmotion: resolving metadata for "${{comp.id}}"`);
    const baseProps = comp.defaultProps ?? {{}};
    const extraProps = getRemotionEnvironment().isPlayer ? {{}} : (getInputProps() ?? {{}});
    const mergedProps = {{ ...baseProps, ...extraProps }};
    try {{
      const resolved = await Promise.resolve(
        Internals.resolveVideoConfig({{
          calculateMetadata: comp.calculateMetadata,
          compositionDurationInFrames: comp.durationInFrames ?? null,
          compositionFps: comp.fps ?? null,
          compositionHeight: comp.height ?? null,
          compositionWidth: comp.width ?? null,
          signal: new AbortController().signal,
          inputProps: mergedProps,
          defaultProps: baseProps,
          compositionId: comp.id,
          schema: comp.schema ?? null,
        }})
      );
      const {{ props, defaultProps, ...rest }} = resolved;
      return {{
        ...rest,
        serializedResolvedPropsWithCustomSchema: NoReactInternals.serializeJSONWithSpecialTypes({{
          data: props,
          indent: undefined,
          staticBase: null,
        }}).serializedString,
        serializedDefaultPropsWithCustomSchema: NoReactInternals.serializeJSONWithSpecialTypes({{
          data: defaultProps,
          indent: undefined,
          staticBase: null,
        }}).serializedString,
      }};
    }} finally {{
      continueRender(h);
    }}
  }};

  window.getStaticCompositions = () =>
    Promise.all(getRegisteredCompositions().map(resolveComposition));

  window.remotion_getCompositionNames = () =>
    getRegisteredCompositions().map((c) => c.id);

  window.remotion_calculateComposition = async (id) => {{
    const comps = getRegisteredCompositions();
    const target = comps.find((c) => c.id === id);
    if (!target) {{
      const avail = comps.map((c) => c.id).join(", ");
      throw new Error(`rsmotion: composition "${{id}}" not found (available: ${{avail}})`);
    }}
    return resolveComposition(target);
  }};

  window.remotion_setBundleMode = (nextMode) => {{
    activeMode = nextMode;
    const h = delayRender("rsmotion: switching bundle mode");
    Internals.waitForRoot((Root) => {{
      renderMode(Root, activeMode);
      requestAnimationFrame(() => continueRender(h));
    }});
  }};
}}
"#
    ))
}

#[derive(Debug)]
struct RsmotionBundlerPlugin {
    virtual_entry_id: String,
    virtual_entry_source: String,
    remotion_root: PathBuf,
    out_dir: PathBuf,
    enable_tailwind: bool,
    tailwind_used: Arc<AtomicBool>,
    singleton_aliases: HashMap<&'static str, String>,
}

impl Plugin for RsmotionBundlerPlugin {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("rsmotion-bundler-plugin")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load
    }

    fn resolve_id_meta(&self) -> Option<PluginHookMeta> {
        Some(PluginHookMeta {
            order: Some(PluginOrder::Pre),
        })
    }

    async fn resolve_id(
        &self,
        _ctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> HookResolveIdReturn {
        if args.is_entry && args.specifier == self.virtual_entry_id {
            return Ok(Some(HookResolveIdOutput::from_id(
                self.virtual_entry_id.clone(),
            )));
        }
        if args.specifier == SETUP_ENV_ID {
            return Ok(Some(HookResolveIdOutput::from_id(SETUP_ENV_ID)));
        }
        if let Some(target) = self.singleton_aliases.get(args.specifier) {
            return Ok(Some(HookResolveIdOutput::from_id(target.clone())));
        }
        Ok(None)
    }

    fn load_meta(&self) -> Option<PluginHookMeta> {
        Some(PluginHookMeta {
            order: Some(PluginOrder::Pre),
        })
    }

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        if args.id == self.virtual_entry_id {
            return Ok(Some(HookLoadOutput {
                code: self.virtual_entry_source.clone().into(),
                module_type: Some(ModuleType::Js),
                ..Default::default()
            }));
        }
        if args.id == SETUP_ENV_ID {
            return Ok(Some(HookLoadOutput {
                code: SETUP_ENV_SOURCE.into(),
                module_type: Some(ModuleType::Js),
                ..Default::default()
            }));
        }

        let clean_path = clean_specifier_path(args.id);
        let p = Path::new(clean_path);
        let is_css = p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("css"));
        if is_css && p.is_file() {
            let (css, used_tw) =
                tailwind::process_css_file(p, &self.remotion_root, self.enable_tailwind)?;
            if used_tw {
                self.tailwind_used.store(true, Ordering::Relaxed);
            }
            let is_module = p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".module.css"));
            let label = p
                .strip_prefix(&self.remotion_root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            let js = tailwind::css_to_style_loader_js(&css, &label, is_module)?;
            return Ok(Some(HookLoadOutput {
                code: js.into(),
                module_type: Some(ModuleType::Js),
                ..Default::default()
            }));
        }

        if has_asset_extension(clean_path) && p.is_file() {
            let bytes = fs::read(p).with_context(|| format!("reading asset {}", p.display()))?;
            let digest: String = Sha256::digest(&bytes)
                .iter()
                .take(4)
                .map(|b| format!("{b:02x}"))
                .collect();
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("asset")
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("bin");
            let file_name = format!("{stem}-{digest}.{ext}");
            let assets_dir = self.out_dir.join("assets");
            fs::create_dir_all(&assets_dir)?;
            fs::write(assets_dir.join(&file_name), &bytes)?;
            let code = format!("export default \"./assets/{file_name}\";\n");
            return Ok(Some(HookLoadOutput {
                code: code.into(),
                module_type: Some(ModuleType::Js),
                ..Default::default()
            }));
        }

        Ok(None)
    }
}

/// Runs Rolldown over `user_entry` and writes `<out_dir>/bundle.js`. Returns whether Tailwind was invoked.
pub async fn run_rolldown_bundle(
    user_entry: &Path,
    remotion_root: &Path,
    out_dir: &Path,
    enable_tailwind: bool,
) -> Result<(u64, bool)> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("creating bundle output dir {}", out_dir.display()))?;

    let virtual_entry_id = remotion_root
        .join(VIRTUAL_ENTRY_FILENAME)
        .to_string_lossy()
        .into_owned();
    let virtual_entry_source = build_virtual_entry_source(user_entry)?;
    let singleton_aliases = collect_singleton_aliases(remotion_root);
    let tailwind_used = Arc::new(AtomicBool::new(false));

    let plugin = RsmotionBundlerPlugin {
        virtual_entry_id: virtual_entry_id.clone(),
        virtual_entry_source,
        remotion_root: remotion_root.to_path_buf(),
        out_dir: out_dir.to_path_buf(),
        enable_tailwind,
        tailwind_used: Arc::clone(&tailwind_used),
        singleton_aliases,
    };

    let mut define: FxIndexMap<String, String> = FxIndexMap::default();
    define.insert(
        "process.env.NODE_ENV".to_string(),
        "\"production\"".to_string(),
    );
    define.insert("global".to_string(), "globalThis".to_string());

    let bundler_options = BundlerOptions {
        input: Some(vec![InputItem {
            name: Some("bundle".to_string()),
            import: virtual_entry_id,
        }]),
        cwd: Some(remotion_root.to_path_buf()),
        dir: Some(out_dir.to_string_lossy().into_owned()),
        format: Some(OutputFormat::Iife),
        platform: Some(Platform::Browser),
        code_splitting: Some(CodeSplittingMode::Bool(false)),
        entry_filenames: Some("bundle.js".to_string().into()),
        define: Some(define),
        tsconfig: Some(TsConfig::Auto(true)),
        transform: Some(BundlerTransformOptions {
            jsx: Some(Either::Right(JsxOptions {
                runtime: Some("automatic".to_string()),
                development: Some(false),
                ..Default::default()
            })),
            ..Default::default()
        }),
        resolve: Some(ResolveOptions {
            extensions: Some(
                [
                    ".ts", ".tsx", ".web.js", ".js", ".jsx", ".mjs", ".cjs", ".json",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
            ),
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut bundler: Bundler = BundlerBuilder::default()
        .with_options(bundler_options)
        .with_plugins(vec![Plugin::new_shared(plugin)])
        .build()
        .map_err(|errs| anyhow::anyhow!("rolldown bundler init failed: {errs}"))?;

    let output = bundler
        .write()
        .await
        .map_err(|errs| anyhow::anyhow!("rolldown bundle failed: {errs}"))?;

    let _ = output;
    let bundle_js_path = out_dir.join("bundle.js");
    let bundle_bytes = fs::metadata(&bundle_js_path)
        .with_context(|| format!("missing output bundle {}", bundle_js_path.display()))?
        .len();

    Ok((bundle_bytes, tailwind_used.load(Ordering::Relaxed)))
}
