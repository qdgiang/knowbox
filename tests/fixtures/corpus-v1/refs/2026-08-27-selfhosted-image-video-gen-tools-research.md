# Deep research: open-source self-hosted image/video generation workflow tools — current state for a Mac↔Fedora↔ai-box.vn deployment

**Date:** 2026-08-27 · **Method:** deep-research workflow (103 agents: 5 search angles → 21 sources fetched → 85 claims extracted → top 25 adversarially verified 3-vote, 21 confirmed / 4 refuted → synthesized) · **Findings current as of 2026-08-27.**

## Research question

Current state (August 2026) of open-source, self-hostable image/video generation workflow tools, evaluated for: a headless Fedora 44 laptop server (CPU-only i5-9300H, no GPU) reached over Tailscale from a MacBook, with generation compute provided by a remote OpenAI-compatible API proxy (`https://api.ai-box.vn/v1` — confirmed in KB kb-0018 pool drift check 2026-08-26: `qwen-image-2.0/3.0/3.0-pro` image models, `wan2.7-*`/`wan3.0-video*` video+image, `happyhorse-*` video edit/i2v/r2v/t2v) rather than local inference.

Three goals: (1) tool landscape, (2) Mac desktop ↔ headless Linux server client/server split, (3) external-API backend support (decisive criterion).

## Executive summary

**The assumed stack is verified as the best option:** headless ComfyUI server on the Fedora laptop + Mac client (official Comfy Desktop **"Remote" instance type**, shipped in the June 2026 desktop rewrite — or plain browser over Tailscale) + a small community custom node pointing at `https://api.ai-box.vn/v1` for generation. Because all inference is offloaded to the remote proxy, the CPU-only constraint is essentially irrelevant, and ComfyUI's best-in-class HTTP/WebSocket API remains available for automation.

What does NOT work:
- **ComfyUI native Partner Nodes** (OpenAI/Kling/Luma etc.) require a Comfy account with prepaid credits routed through Comfy's billing proxy, and as of 2026-08 do NOT support bring-your-own-key or custom base URLs (open request Comfy-Org/ComfyUI_frontend#7850). Cannot point at ai-box.vn; a custom node is required.
- **Native video workflows** (Wan 2.2, Hunyuan Video, Mochi, LTX-Video) are hard-wired to locally downloaded model files — unusable on a GPU-less server. Wan API-node variants cover only Wan 2.1/2.5/2.6/2.7 via Comfy's cloud.
- **SD.Next (vladmandic)** and **SwarmUI**: both cover image AND video generation with active maintenance, but both are effectively hard-wired to local inference backends → disqualified on the decisive criterion (one third-party extension `BinaryQuantumSoul/sdnext-remote` exists for SD.Next but was not evaluated/verified).

**Principal risk = maturity:** the two verified OpenAI-compatible proxy nodes are tiny early-stage projects (0 stars, ~3-5 commits), and neither is verified for VIDEO endpoints. `wan3.0-video`/`wan2.7`/`happyhorse-*` support will likely require extending a node or writing a new one.

## Findings (all adversarially verified, 3-vote)

### 1. Mac ↔ headless-Linux split is officially supported (high confidence)
Comfy Desktop (cross-platform, June 2026 rewrite into Comfy-Org/Comfy-Desktop) offers a **Remote** instance type — one of four official instance types: "Connects to a ComfyUI instance running on another machine on your network. Configure the remote host and port during setup" (docs.comfy.org instance-management). macOS supported.
- Networking: server binds `127.0.0.1` by default (`cli_args.py`); needs `--listen` (CLI/manual install) or Desktop Settings → Server Configuration → Network → 0.0.0.0. Default ports: **8188** CLI/portable, **8000** Desktop; override with `--port`.
- ComfyUI has **no built-in auth** → Tailscale is the appropriate exposure mechanism (practitioner pattern documented at justiceshultz.com: `--listen --port 8188`, connect browser at Tailscale IP:port).
- Earlier "Desktop cannot be a remote client" claims were REFUTED 0-3 as stale (predate the June 2026 release).
- Carry-caveats: remote instances expose fewer tabs/settings; intermittent remote-connection bugs reported (desktop#1058, frontend#976); non-localhost connections involve a platform.comfy.org login/API-key flow.
- Alternative: run Comfy-Org/ComfyUI_frontend locally on Mac with `DEV_SERVER_COMFYUI_URL` pointed at the remote backend (practitioner pattern).
Sources: docs.comfy.org/installation/desktop/usage/instance-management; docs.comfy.org/development/comfyui-server/startup-flags; comfyui-wiki.com LAN FAQ.

### 2. ComfyUI API is best-in-class for automation (high confidence)
Full HTTP/WebSocket API: GET/POST `/prompt`, `/queue`, `/history`, `/view` + WebSocket messages; per-client `client_id` routing enables multiple front-ends against one backend. Headless API-only operation confirmed (Comfy-Org/ComfyUI#5433). Clore.ai comparison rates API/Automation: ComfyUI 5/5 vs SD WebUI 3/5, Forge 3/5, Fooocus 2/5; A1111's API noted as poorly maintained. Qualifications: single execution queue per instance (queued, not parallel), no built-in auth.
Sources: docs.comfy.org/development/comfyui-server/comms_routes; docs.clore.ai comparison.

### 3. Decisive criterion met: custom nodes for arbitrary OpenAI-compatible base URLs (high confidence)
Two verified nodes delegate image generation to an arbitrary OpenAI-compatible base URL (e.g. `https://api.ai-box.vn/v1`), with configurable endpoint path, headers, and body:
1. **wennatre/ComfyUI-OpenAI-Compatible-Image** (MIT, ~May 2026): `api_base` is a node-UI-editable STRING (default `https://api.openai.com/v1`); POSTs `{api_base}{endpoint_path}` (default `/images/generations`) with Bearer auth; `extra_headers_json`/`extra_body_json` merge into the request; tolerant response parsing (data[].b64_json/url, alternative keys image_url/output_url/result_url/download_url, nested containers, top-level images/outputs/results arrays) — verified in source.
2. **kulefanle/comfyui_proxy_gpt_image_node**: `api_route` combo of `/v1/images/generations` (default) or `/v1/chat/completions`, confirmed in `__init__.py`; parses b64_json/URL/data-URL/chat-completions responses into IMAGE tensors.
Independent corroboration: official Comfy Registry listings "GPT-Image (OpenAI-compatible)" (2026-07-20), AV_OpenAIApi (configurable endpoint + key), APIYI docs. Caveat: both verified nodes are tiny (0 stars, 3-5 commits; kulefanle has no visible LICENSE) — mechanism solid, maintenance weak.
Sources: github.com/wennatre/ComfyUI-OpenAI-Compatible-Image; github.com/kulefanle/comfyui_proxy_gpt_image_node; registry.comfy.org.

### 4. Native Partner Nodes ≠ BYOK (high confidence)
Partner Nodes officially support remote-API generation (OpenAI, Kling, Luma…) but require Comfy login + prepaid non-refundable credits routed through api.comfy.org. FAQ on own keys: "still in the testing phase and do not support this feature yet, but we have considered adding it"; launch blog's "Bring your own API key" sits under "Coming soon". Request #7850 still open (asks through Apr 2026). Third-party bypass projects (comfy-api-liberation, comfyui-byokey) exist specifically to escape the proxy — confirming calls route through Comfy's billing. Comfy Cloud BYOK exists only for Runway/Gemini on the hosted web platform.
Sources: docs.comfy.org/tutorials/partner-nodes/overview; blog.comfy.org/p/comfyui-native-api-nodes.

### 5. Native video pipelines are local-model-bound (high confidence)
ComfyUI native video coverage is broad and fast: Wan 2.2 (14B T2V/I2V/FLF2V, 5B TI2V), Hunyuan Video, Mochi, LTX-Video, day-0/day-1 support pattern. But every native workflow requires local files in `models/diffusion_models/`, `models/vae/`, `models/text_encoders/` — no API-delegation node in any official graph; the only remote touch is "Run on Comfy Cloud". Wan API nodes cover only 2.1/2.5/2.6/2.7 closed models via Comfy's backend. On a GPU-less server these pipelines are unusable unless offloaded via custom node.
Sources: docs.comfy.org/tutorials/video/wan/wan2_2; blog.comfy.org/p/wan22-day-0-support-in-comfyui.

### 6. SD.Next — disqualified (high confidence)
Apache-2.0, active through 2026 (releases to 2026-07-07), covers T2I/I2I/T2V/I2V, CPU-capable backends. But all compute platforms are local (CUDA, ROCm, ZLUDA, OneAPI/IPEX, OpenVINO, DirectML, MPS, ONNX/Olive + CPU-only); models downloaded locally on demand. Its only OpenAI-compatible feature runs in the REVERSE direction (SD.Next serving an OpenAI-compatible API outward). Caveat: third-party `BinaryQuantumSoul/sdnext-remote` exists (unverified).
Sources: github.com/vladmandic/sdnext; vladmandic.github.io/sdnext.

### 7. SwarmUI — image+video but external-API delegation unproven (medium confidence)
SwarmUI (formerly StableSwarmUI, v0.9.8 beta, MIT; ComfyUI GPL-3.0 as backend engine → GPL obligations when bundled). Covers SD, SDXL, Flux 1/2, Z-Image, Qwen Image (+Edit), Wan 2.1 T2V/I2V, Hunyuan Video, MiniMax H3, LTX-2. Form-based Generate tab with ComfyUI backend. No confirmed support for delegating generation to external OpenAI-compatible APIs (a blanket "no external API support" claim was REFUTED 1-2, so status is unproven, not disproven; mentioned remote options concern renting GPU hosts).
Sources: github.com/mcmonkeyprojects/SwarmUI; techtactician.com comparison.

## Landscape snapshot (from search/fetch; licenses per earngenix 2026 list)
ComfyUI GPL-3.0 node/graph · Fooocus GPL-3.0 minimal form · Forge AGPL-3.0 A1111-style · Automatic1111 AGPL-3.0 form (API poorly maintained) · InvokeAI Apache-2.0 canvas/form · SD.Next Apache-2.0 · SwarmUI MIT. Landscape gaps: Fooocus/InvokeAI/Open WebUI images tab/Pinokio/Easy Diffusion were not deeply verified, but the local-inference-first pattern generalizes.

## Notable side findings
- **`happyhorse-*`, `wan2.7-*`, `wan3.0-video*` are proxy-side endpoint names, not open-weights releases** — official Alibaba Wan open weights stop at Wan 2.2; checkpoints advertised as "Wan 2.7" are not official releases (localaimaster.com, Apr 2026). Relevant when reading ai-box.vn's model list.
- Wan 2.2 itself is Apache-2.0 and runs from ~8GB VRAM with GGUF quantization (thundercompute, 2026-08-20) — only relevant if a GPU ever appears in the stack.

## Caveats
1. **Proxy node maturity**: both verified OpenAI-compatible nodes are tiny (0 stars, 3-5 commits, one license-less). Expect to vendor/fork.
2. **Video endpoint gap**: both nodes target image endpoints (`/images/generations` or `/chat/completions`). Whether ai-box.vn's video models use OpenAI-style schemas, sync vs async job polling, and response shapes is UNVERIFIED — check https://api.ai-box.vn/docs; likely requires extending a node or writing a new one.
3. **Networking**: no built-in auth in ComfyUI; 0.0.0.0 listen breaks ComfyUI-Manager "local mode" (#2854); Desktop remote connection involves platform.comfy.org login flow.
4. **Time-sensitivity**: Partner-Node BYOK could land any release; Desktop Remote is 2 months old with intermittent bugs; registry listings churn.

## Open questions
1. What request/response contract do ai-box.vn's video endpoints actually use (sync vs async polling, schema), and can wennatre's node be trivially extended to cover them?
2. Do more established registry nodes (GPT-Image OpenAI-compatible, AV_OpenAIApi, ComfyUI-Custom-Scripts, comfy-api-liberation) already handle generic /images AND /video with better maintenance?
3. Would an API-first front end (Open WebUI images tab, n8n calling /images and /video directly) be simpler than ComfyUI's node graph when 100% of compute is remote?
4. Will ComfyUI ship BYOK/custom-base-URL for Partner Nodes (#7850), making the custom-node approach obsolete?

## Recommended next steps
1. Install headless ComfyUI on fedora-server (uv venv or portable; `--listen 0.0.0.0 --port 8188`, systemd unit), expose only via Tailscale (no auth needed then).
2. Probe ai-box.vn video endpoint contract (per kb-0070: docs first, then a smoke test with one model) to determine sync/async + schema.
3. Vendor wennatre/ComfyUI-OpenAI-Compatible-Image; point api_base at https://api.ai-box.vn/v1; test qwen-image-3.0 end-to-end.
4. Extend/fork the node for video endpoints based on step 2.
5. On Mac: Comfy Desktop → Remote instance (or browser via Tailscale) as the daily driver.

## Sources (verified in-line during research)
Primary: docs.comfy.org (instance-management, startup-flags, comms_routes, partner-nodes overview, wan2_2 tutorial, desktop faq) · github.com/vladmandic/sdnext · github.com/mcmonkeyprojects/SwarmUI · github.com/wennatre/ComfyUI-OpenAI-Compatible-Image · github.com/kulefanle/comfyui_proxy_gpt_image_node · registry.comfy.org · blog.comfy.org. Secondary: docs.clore.ai comparison · comfyui-wiki.com · comfy.icu · techtactician.com · builderai.tools · localaimaster.com · thundercompute.com · forum.comfy.org · justiceshultz.com (ComfyUI+Tailscale writeup).
