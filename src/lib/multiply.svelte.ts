import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, type DocType } from "$lib/api";
import { assistant } from "$lib/assistant.svelte";

// ---------------------------------------------------------------------------
// Content multiplication — re-shape a FINISHED source document into a
// platform-native draft of a chosen target type. Headless: no editor, no chat.
// Reuses the chat `assistant:*` events (filtered by stream id) and the single
// AiState slot, so a generation aborts any in-flight chat/inline/expand stream
// and vice versa. See ai.rs::start_content_multiply_stream.
//
// `generate()` resolves with the full draft once the stream completes, so the
// caller can create a new document from it. The single-flight `isGenerating`
// flag is the frontend half of the sequential contract: the orchestrator must
// await each target before starting the next (the backend slot allows only one).
// ---------------------------------------------------------------------------

interface StreamToken {
  id: string;
  text: string;
}
interface StreamDone {
  id: string;
  /** True when the stream was aborted (superseded by another AI action). */
  aborted?: boolean;
}
interface StreamError {
  id: string;
  message: string;
}

class MultiplyController {
  /** True while a draft is being generated. */
  isGenerating = $state(false);

  /** Id of the in-flight stream; events with any other id are stale. */
  private activeStreamId: string | null = null;
  private streamed = "";
  private resolve: ((text: string) => void) | null = null;
  private reject: ((err: Error) => void) | null = null;
  private unlisteners: UnlistenFn[] = [];
  private listening = false;

  /** Register the Tauri event listeners once. Mirrors IdeaExpandController.init. */
  async init() {
    if (this.listening) return;
    this.listening = true;
    this.unlisteners = await Promise.all([
      listen<StreamToken>("assistant:token", (e) => {
        if (e.payload.id === this.activeStreamId) this.streamed += e.payload.text;
      }),
      listen<StreamDone>("assistant:done", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        if (e.payload.aborted) {
          // another AI action took the stream slot — the draft is truncated,
          // never save it as a document
          this.fail(new Error("generation was interrupted by another AI action"));
          return;
        }
        this.finish(this.streamed.trim());
      }),
      listen<StreamError>("assistant:error", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        this.fail(new Error(e.payload.message));
      }),
    ]);
  }

  destroy() {
    this.unlisteners.forEach((fn) => fn());
    this.unlisteners = [];
    this.listening = false;
  }

  /** Adapt `sourceContent` into a `targetLabel` draft of type `target`. Resolves
      with the draft markdown when the stream completes; rejects on error. Only
      one generation at a time — callers run targets sequentially. */
  generate(sourceContent: string, target: DocType, targetLabel: string): Promise<string> {
    if (this.isGenerating) {
      return Promise.reject(new Error("a generation is already running"));
    }
    if (!assistant.isConfigured) {
      return Promise.reject(new Error("Add an AI API key in Settings to multiply documents."));
    }
    this.streamed = "";
    this.isGenerating = true;
    this.activeStreamId = crypto.randomUUID();
    const promise = new Promise<string>((resolve, reject) => {
      this.resolve = resolve;
      this.reject = reject;
    });
    // Honor the user's selected model (falls back to the provider default only
    // when none is set) — never silently use the expensive default model.
    api
      .sendContentMultiply(
        this.activeStreamId,
        assistant.settings.provider,
        assistant.settings.model || null,
        sourceContent,
        target,
        targetLabel,
        assistant.settings.voice || null,
      )
      .catch((e) => this.fail(e instanceof Error ? e : new Error(String(e))));
    return promise;
  }

  /** Abort the in-flight generation (user pressed Cancel). Stops the backend
      stream and rejects the pending promise so the orchestrator unwinds. */
  cancel() {
    if (!this.isGenerating) return;
    void api.stopAssistant();
    this.fail(new Error("Multiply canceled"));
  }

  private finish(text: string) {
    const resolve = this.resolve;
    this.cleanup();
    resolve?.(text);
  }

  private fail(err: Error) {
    const reject = this.reject;
    this.cleanup();
    reject?.(err);
  }

  private cleanup() {
    this.activeStreamId = null;
    this.streamed = "";
    this.resolve = null;
    this.reject = null;
    this.isGenerating = false;
  }
}

export const multiply = new MultiplyController();
