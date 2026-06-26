import { api } from "$lib/api";
import { assistant } from "$lib/assistant.svelte";
import { formatError } from "$lib/formatError";
import { HeadlessStream } from "$lib/headlessStream.svelte";

// ---------------------------------------------------------------------------
// Idea expansion — turn a captured idea fragment into a full draft of a chosen
// document type. Headless: no editor, no chat. Reuses the chat `assistant:*`
// events (filtered by stream id) and the single AiState slot, so starting an
// expansion aborts any in-flight chat/inline stream and vice versa — same
// mutual-exclusion model as inline edit. See ai.rs::start_expand_stream.
//
// `expand()` resolves with the full generated draft once the stream completes,
// so the caller can create + open a new document from it.
// ---------------------------------------------------------------------------

class IdeaExpandController extends HeadlessStream {
  /** True while a draft is being generated — drives the Inbox spinner. */
  isExpanding = $state(false);
  /** True when the last run ended because the user canceled it (vs. an error),
      so the caller can stay quiet instead of toasting a failure. */
  canceled = false;

  get isBusy(): boolean {
    return this.isExpanding;
  }

  protected label() {
    return "expansion";
  }

  protected setBusy(on: boolean) {
    this.isExpanding = on;
  }

  protected onCancel() {
    this.canceled = true;
  }

  /** Expand `idea` into a `targetLabel` draft. Resolves with the draft markdown
      when the stream completes; rejects on error. Only one expansion at a time. */
  expand(idea: string, targetLabel: string): Promise<string> {
    if (!assistant.isConfigured) {
      return Promise.reject(new Error("Add an AI API key in Settings to expand ideas."));
    }
    this.canceled = false;
    return this.start((streamId) =>
      api
        .sendIdeaExpand(
          streamId,
          assistant.settings.provider,
          assistant.settings.model || null,
          idea,
          targetLabel,
          assistant.settings.voice || null,
        )
        .catch((e) => {
          throw new Error(formatError(e));
        }),
    );
  }
}

export const ideaExpand = new IdeaExpandController();
