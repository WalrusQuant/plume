import { api, type DocType } from "$lib/api";
import { assistant } from "$lib/assistant.svelte";
import { formatError } from "$lib/formatError";
import { HeadlessStream } from "$lib/headlessStream.svelte";

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

class MultiplyController extends HeadlessStream {
  /** True while a draft is being generated. */
  isGenerating = $state(false);

  get isBusy(): boolean {
    return this.isGenerating;
  }

  protected label() {
    return "generation";
  }

  protected setBusy(on: boolean) {
    this.isGenerating = on;
  }

  /** Adapt `sourceContent` into a `targetLabel` draft of type `target`. Resolves
      with the draft markdown when the stream completes; rejects on error. Only
      one generation at a time — callers run targets sequentially. */
  generate(sourceContent: string, target: DocType, targetLabel: string): Promise<string> {
    if (!assistant.isConfigured) {
      return Promise.reject(new Error("Add an AI API key in Settings to multiply documents."));
    }
    return this.start((streamId) =>
      api
        .sendContentMultiply(
          streamId,
          assistant.settings.provider,
          assistant.settings.model || null,
          sourceContent,
          target,
          targetLabel,
          assistant.settings.voice || null,
        )
        .catch((e) => {
          throw new Error(formatError(e));
        }),
    );
  }
}

export const multiply = new MultiplyController();
