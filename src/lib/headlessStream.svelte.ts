import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "$lib/api";

// ---------------------------------------------------------------------------
// Shared base for headless AI stream controllers (idea expand, content
// multiply). Both wrap a single-use Tauri stream that resolves with the full
// generated text on completion. They share the same event plumbing (token
// accumulation, done/error handling, single-flight guard, abort), differing
// only in the API call and the busy-flag name.
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

/** Human-readable label used in messages ("expansion", "generation", etc.). */
export type StreamLabel = string;

/** Max idle time (ms) with no token/error/done before a stream is treated as hung. */
const STREAM_IDLE_TIMEOUT_MS = 90_000;

export abstract class HeadlessStream {
  /** True while a stream is active — drives the UI spinner. Subclass-owned. */
  abstract readonly isBusy: boolean;

  private activeStreamId: string | null = null;
  private streamed = "";
  private resolve: ((text: string) => void) | null = null;
  private reject: ((err: Error) => void) | null = null;
  private unlisteners: UnlistenFn[] = [];
  private listening = false;
  /** Idle watchdog: cleared on each token, fires a timeout on expiry. */
  private idleTimer: ReturnType<typeof setTimeout> | null = null;

  /** Register the Tauri event listeners once. */
  async init() {
    if (this.listening) return;
    this.listening = true;
    this.unlisteners = await Promise.all([
      listen<StreamToken>("assistant:token", (e) => {
        if (e.payload.id === this.activeStreamId) {
          this.streamed += e.payload.text;
          this.resetIdleTimer();
        }
      }),
      listen<StreamDone>("assistant:done", (e) => {
        if (e.payload.id !== this.activeStreamId) return;
        if (e.payload.aborted) {
          this.fail(new Error(`${this.label()} was interrupted by another AI action`));
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

  /** Human-readable label for messages ("expansion", "generation", etc.). */
  protected abstract label(): StreamLabel;
  /** Flip the busy flag on/off. */
  protected abstract setBusy(on: boolean): void;
  /** Hook for subclass state reset on cancel (e.g. setting a `canceled` flag). */
  protected onCancel(): void {}

  /**
   * Start a stream. `invoke` takes the streamId and kicks off the backend; it
   * should reject on error. Resolves with the full streamed text on completion,
   * rejects on error/abort. Single-flight: a second call while busy rejects.
   */
  protected start(invoke: (streamId: string) => Promise<void>): Promise<string> {
    if (this.isBusy) {
      return Promise.reject(new Error(`a ${this.label()} is already running`));
    }
    this.streamed = "";
    this.setBusy(true);
    this.activeStreamId = crypto.randomUUID();
    const promise = new Promise<string>((resolve, reject) => {
      this.resolve = resolve;
      this.reject = reject;
    });
    invoke(this.activeStreamId).catch((e) => this.fail(e instanceof Error ? e : new Error(String(e))));
    this.resetIdleTimer();
    return promise;
  }

  /** Reset the idle watchdog — called on each token and at stream start. */
  private resetIdleTimer() {
    if (this.idleTimer) clearTimeout(this.idleTimer);
    this.idleTimer = setTimeout(() => {
      // No token/error/done arrived in time — treat as a hung connection.
      void api.stopAssistant();
      this.fail(new Error(`${this.capitalizedLabel()} timed out — no response from the provider`));
    }, STREAM_IDLE_TIMEOUT_MS);
  }

  /** Abort the in-flight stream (user pressed Cancel). */
  cancel() {
    if (!this.isBusy) return;
    this.onCancel();
    void api.stopAssistant();
    this.fail(new Error(`${this.capitalizedLabel()} canceled`));
  }

  protected finish(text: string) {
    const resolve = this.resolve;
    this.cleanup();
    resolve?.(text);
  }

  protected fail(err: Error) {
    const reject = this.reject;
    this.cleanup();
    reject?.(err);
  }

  private cleanup() {
    if (this.idleTimer) {
      clearTimeout(this.idleTimer);
      this.idleTimer = null;
    }
    this.activeStreamId = null;
    this.streamed = "";
    this.resolve = null;
    this.reject = null;
    this.setBusy(false);
  }

  private capitalizedLabel(): string {
    return this.label().charAt(0).toUpperCase() + this.label().slice(1);
  }
}
