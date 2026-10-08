---
id: kb-0079
title: "Self-hosted image/video gen tools for Mac↔Fedora↔ai-box.vn: section map for refs/2026-08-27-selfhosted-image-video-gen-tools-research.md"
type: reference
domain: tooling
tags: [comfyui, image-generation, video-generation, ai-box, tailscale]
aliases: [comfyui research, video gen workflow tools, image gen self-hosted]
triggers:
  - "setting up ComfyUI or any image/video generation workflow on fedora-server"
  - "connecting a Mac client to the headless Fedora server for image/video gen"
  - "choosing or wiring custom nodes that call ai-box.vn image/video endpoints"
status: active
confidence: high
created: 2026-08-27
updated: 2026-08-27
supersedes: []
related: [kb-0018, kb-0070, kb-0017]
source: "deep-research workflow wf_77c77772-d54, 2026-08-27"
summary: "Adversarially-verified 2026-08 research: best stack = headless ComfyUI on Fedora + Comfy Desktop Remote/browser over Tailscale + OpenAI-compatible custom node → ai-box.vn; findings + sources + open questions."
---
Deep-research report (103 agents, 21 sources, 25 claims 3-vote adversarially verified, findings current 2026-08-27) answering: which open-source self-hosted image/video gen tool fits a GPU-less Fedora server driven from a MacBook, with all inference via the ai-box.vn OpenAI-compatible proxy?

⚠ Fast-moving space — Partner-Node BYOK, Desktop Remote bugs, and registry listings can change; re-check before trusting beyond ~3 months. Full doc: `refs/2026-08-27-selfhosted-image-video-gen-tools-research.md`.

## Section map
- **Executive summary** — recommended stack (ComfyUI + Desktop Remote/browser + custom node), what's disqualified, principal risk = proxy-node maturity.
- **Findings 1-2** — Mac↔server split officially supported (Remote instance type, ports 8188/8000, `--listen`, no auth → Tailscale); ComfyUI HTTP/WS API best-in-class (routes, client_id).
- **Finding 3** — decisive criterion met: wennatre/ComfyUI-OpenAI-Compatible-Image + kulefanle/comfyui_proxy_gpt_image_node (configurable base URL/path/headers/body; both tiny/early-stage).
- **Finding 4** — native Partner Nodes are NOT BYOK (prepaid Comfy credits; #7850 open).
- **Finding 5** — native video workflows (Wan 2.2, Hunyuan, Mochi, LTX) hard-wired to local model files.
- **Findings 6-7** — SD.Next disqualified (local-only; note `sdnext-remote` extension); SwarmUI external-API delegation unproven.
- **Landscape snapshot** — licenses/UI paradigm per tool.
- **Notable side findings** — "wan2.7"/"wan3.0-video" are proxy endpoint names, not official open-weights (official Wan stops at 2.2).
- **Caveats** — proxy node maturity, VIDEO endpoint gap (unverified contract), networking, time-sensitivity.
- **Open questions** — ai-box.vn video endpoint contract; more mature registry nodes; API-first alternatives; BYOK landing.
- **Recommended next steps** — install plan, endpoint probe, node vendoring/extension.
- **Sources** — primary (docs.comfy.org, GitHub repos, registry) vs secondary.

## Related
- [[kb-0018]] — ai-box.vn model pool incl. the image/video endpoints this research targets
- [[kb-0070]] — consult api.ai-box.vn/docs before wiring anything to the proxy
- [[kb-0017]] — Hermes gateway + proxy quirks (same provider)
