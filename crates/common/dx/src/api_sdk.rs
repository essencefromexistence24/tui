//! TypeScript SDK generator for the dx-tui HTTP API.
//!
//! Generates a TypeScript client class that wraps all REST endpoints,
//! SSE streaming, and WebSocket communication.

#[allow(dead_code)]
pub fn generate_typescript_sdk() -> String {
	r#"// ── dx-tui API Client SDK ──────────────────────────────────────────────
// Auto-generated. Do not edit manually.
// Compatible with: dx-tui v26.2.x

interface ApiConfig {
  /** Default: 127.0.0.1 */
  host?: string;
  /** Default: 10245 */
  port?: number;
  /** Bearer token for authenticated requests */
  authToken?: string;
  /** Request timeout in ms (default: 30000) */
  timeout?: number;
}

interface ChatMessage {
  role: "user" | "assistant" | "system";
  content: string;
}

interface ChatOptions {
  model?: string;
  system?: string;
  mode?: "ask" | "write" | "plan" | "goal" | "agent";
  history?: ChatMessage[];
  stream?: boolean;
}

interface ChatTurn {
  text: string;
  tool_calls: ToolCall[];
  usage: TokenUsage;
}

interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

interface TokenUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

interface ToolDef {
  name: string;
  description: string;
  parameters: Record<string, unknown>;
}

interface SessionInfo {
  id: string;
  name: string;
  model: string;
  agent_mode: string;
  created_at: string;
  updated_at: string;
  message_count: number;
  total_tokens: number;
  is_archived: boolean;
  tags: string[];
}

interface ProviderInfo {
  id: string;
  name: string;
  kind: string;
  connected: boolean;
}

interface McpServerStatus {
  name: string;
  connected: boolean;
  capabilities: string[];
  tools: ToolDef[];
  error?: string;
}

interface ApiStatus {
  uptime_seconds: number;
  total_requests: number;
  total_errors: number;
  active_connections: number;
  route_counts: Record<string, number>;
  version: string;
}

type SseEvent =
  | { type: "start"; model: string }
  | { type: "delta"; content: string }
  | { type: "turn"; content: string; tool_calls: ToolCall[]; usage: TokenUsage }
  | { type: "error"; message: string }
  | { type: "done" };

export class DxTuiClient {
  private baseUrl: string;
  private authToken?: string;
  private timeout: number;

  constructor(config: ApiConfig = {}) {
    const host = config.host ?? "127.0.0.1";
    const port = config.port ?? 10245;
    this.baseUrl = `http://${host}:${port}`;
    this.authToken = config.authToken;
    this.timeout = config.timeout ?? 30000;
  }

  private headers(): Record<string, string> {
    const h: Record<string, string> = {
      "Content-Type": "application/json",
    };
    if (this.authToken) {
      h["Authorization"] = `Bearer ${this.authToken}`;
    }
    return h;
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown
  ): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);
    try {
      const res = await fetch(`${this.baseUrl}${path}`, {
        method,
        headers: this.headers(),
        body: body ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`HTTP ${res.status}: ${text}`);
      }
      return res.json() as Promise<T>;
    } finally {
      clearTimeout(timer);
    }
  }

  // ── Chat ──────────────────────────────────────────────────────────

  /** Send a message and get a complete response (non-streaming). */
  async chat(
    message: string,
    options: ChatOptions = {}
  ): Promise<ChatTurn> {
    return this.request<ChatTurn>("POST", "/api/chat", {
      model: options.model,
      message,
      system: options.system,
      mode: options.mode,
      history: options.history,
    });
  }

  /** Stream a chat response via SSE. Returns an async generator of SseEvent. */
  async *streamChat(
    message: string,
    options: ChatOptions = {}
  ): AsyncGenerator<SseEvent> {
    const url = new URL(`${this.baseUrl}/api/chat/stream`);
    const res = await fetch(url, {
      method: "POST",
      headers: {
        ...this.headers(),
        Accept: "text/event-stream",
      },
      body: JSON.stringify({
        model: options.model,
        message,
        system: options.system,
        mode: options.mode,
        history: options.history,
      }),
    });
    if (!res.ok || !res.body) {
      throw new Error(`SSE request failed: ${res.status}`);
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";
      for (const line of lines) {
        if (line.startsWith("data: ")) {
          const data = line.slice(6);
          if (data === "[DONE]") {
            yield { type: "done" } as SseEvent;
            return;
          }
          try {
            yield JSON.parse(data) as SseEvent;
          } catch {
            // skip malformed events
          }
        }
      }
    }
  }

  // ── Sessions ──────────────────────────────────────────────────────

  /** List all sessions. */
  async listSessions(): Promise<SessionInfo[]> {
    return this.request<SessionInfo[]>("GET", "/api/sessions");
  }

  /** Get a single session by ID. */
  async getSession(id: string): Promise<SessionInfo> {
    return this.request<SessionInfo>("GET", `/api/sessions/${encodeURIComponent(id)}`);
  }

  /** Delete a session. */
  async deleteSession(id: string): Promise<void> {
    await this.request("DELETE", `/api/sessions/${encodeURIComponent(id)}`);
  }

  /** Archive a session. */
  async archiveSession(id: string): Promise<void> {
    await this.request("POST", `/api/sessions/${encodeURIComponent(id)}/archive`);
  }

  /** Fork a session at a given message index. */
  async forkSession(
    sourceId: string,
    fromMessage: number,
    newName: string
  ): Promise<SessionInfo> {
    return this.request<SessionInfo>("POST", `/api/sessions/fork`, {
      source_id: sourceId,
      from_message: fromMessage,
      new_name: newName,
    });
  }

  // ── Providers ─────────────────────────────────────────────────────

  /** List all configured providers. */
  async listProviders(): Promise<ProviderInfo[]> {
    return this.request<ProviderInfo[]>("GET", "/api/providers");
  }

  /** Refresh the provider model catalog. */
  async refreshProviders(): Promise<void> {
    await this.request("POST", "/api/providers/refresh");
  }

  // ── Tools ─────────────────────────────────────────────────────────

  /** Get all available built-in tool definitions. */
  async listTools(): Promise<ToolDef[]> {
    return this.request<ToolDef[]>("GET", "/api/tools");
  }

  // ── MCP ───────────────────────────────────────────────────────────

  /** Get MCP server statuses. */
  async listMcpServers(): Promise<McpServerStatus[]> {
    return this.request<McpServerStatus[]>("GET", "/api/mcp");
  }

  // ── Status ────────────────────────────────────────────────────────

  /** Get API server status and telemetry. */
  async getStatus(): Promise<ApiStatus> {
    return this.request<ApiStatus>("GET", "/api/status");
  }

  // ── Models ────────────────────────────────────────────────────────

  /** List available models. */
  async listModels(): Promise<string[]> {
    return this.request<string[]>("GET", "/api/models");
  }

  // ── WebSocket ─────────────────────────────────────────────────────

  /**
   * Connect to the WebSocket endpoint for real-time bidirectional
   * communication. Returns a WebSocket (raw browser API).
   */
  ws(): WebSocket {
    const wsUrl = this.baseUrl.replace(/^http/, "ws") + "/ws";
    const headers = this.authToken
      ? { Authorization: `Bearer ${this.authToken}` }
      : undefined;
    const protocols = headers
      ? [JSON.stringify(headers)]
      : undefined;
    return new WebSocket(wsUrl, protocols);
  }
}
"#
	.to_string()
}
