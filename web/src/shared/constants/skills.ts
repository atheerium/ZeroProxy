// Agent Skills metadata — single source of truth for /dashboard/skills page.
// Each skill = 1 raw GitHub URL the user copies and pastes to any AI agent.

const REPO = "atheerium/zeroproxy";
const BRANCH = "main";
const SKILL_PATH = ".agents/skills";

export const SKILLS_REPO_URL = `https://github.com/${REPO}`;
export const SKILLS_RAW_BASE = `https://raw.githubusercontent.com/${REPO}/refs/heads/${BRANCH}/${SKILL_PATH}`;
export const SKILLS_BLOB_BASE = `https://github.com/${REPO}/blob/${BRANCH}/${SKILL_PATH}`;

export interface Skill {
  id: string;
  name: string;
  description: string;
  endpoint: string | null;
  icon: string;
  isEntry?: boolean;
}

export const SKILLS: Skill[] = [
  {
    id: "zeroproxy",
    name: "ZeroProxy (Entry)",
    description: "Setup + index of all capabilities. Start here — covers install, server init, provider apply, combo setup, and wiring every AI coding CLI tool.",
    endpoint: null,
    icon: "hub",
    isEntry: true,
  },
  {
    id: "zeroproxy-chat",
    name: "Chat",
    description: "Chat / code-gen via the OpenAI-compatible API with streaming, multi-modal support, and format translation.",
    endpoint: "/v1/chat/completions",
    icon: "chat",
  },
  {
    id: "zeroproxy-image",
    name: "Image Generation",
    description: "Text-to-image proxied through supported providers — DALL-E, FLUX, SD, and more.",
    endpoint: "/v1/images/generations",
    icon: "image",
  },
  {
    id: "zeroproxy-tts",
    name: "Text-to-Speech",
    description: "OpenAI-compatible TTS routed through supported providers.",
    endpoint: "/v1/audio/speech",
    icon: "record_voice_over",
  },
  {
    id: "zeroproxy-stt",
    name: "Speech-to-Text",
    description: "Transcribe audio via proxied providers — Whisper, Deepgram, and more.",
    endpoint: "/v1/audio/transcriptions",
    icon: "mic",
  },
  {
    id: "zeroproxy-embeddings",
    name: "Embeddings",
    description: "Vectors for RAG / semantic search through supported embedding providers.",
    endpoint: "/v1/embeddings",
    icon: "scatter_plot",
  },
  {
    id: "zeroproxy-web-search",
    name: "Web Search",
    description: "Web search routed through proxied search providers in your combo chain.",
    endpoint: "/v1/search",
    icon: "search",
  },
  {
    id: "zeroproxy-web-fetch",
    name: "Web Fetch",
    description: "URL-to-markdown fetch routed through proxied fetch providers.",
    endpoint: "/v1/web/fetch",
    icon: "language",
  },
  {
    id: "zeroproxy-providers",
    name: "Providers",
    description: "Configure AI providers: OAuth (Claude Code, Codex, Copilot, Cursor), API key (OpenAI, Anthropic, Gemini — 40+), and free tiers (Kiro AI, Vertex AI).",
    endpoint: null,
    icon: "cloud",
  },
  {
    id: "zeroproxy-combos",
    name: "Combos",
    description: "Build ordered fallback chains across providers. ZeroProxy retries each model in sequence, auto-failing over on rate limits and errors.",
    endpoint: null,
    icon: "alt_route",
  },
  {
    id: "zeroproxy-cli-tools",
    name: "CLI Tools",
    description: "Wire Claude Code, Codex, Cursor, Cline, Continue, Roo, Kilo, Copilot, OpenClaw, and more into ZeroProxy with one-click configuration.",
    endpoint: null,
    icon: "terminal",
  },
  {
    id: "zeroproxy-rtk",
    name: "RTK Token Compression",
    description: "Reduce input tokens by 20-40% via runtime token compression of tool-call results. Lower latency and cost on every request.",
    endpoint: null,
    icon: "compress",
  },
];

export function getSkillRawUrl(id: string): string {
  return `${SKILLS_RAW_BASE}/${id}/SKILL.md`;
}

export function getSkillBlobUrl(id: string): string {
  return `${SKILLS_BLOB_BASE}/${id}/SKILL.md`;
}
