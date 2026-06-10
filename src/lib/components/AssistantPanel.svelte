<script lang="ts">
  import { tick } from "svelte";
  import { assistant } from "$lib/assistant.svelte";

  interface Props {
    onApply: (content: string) => void;
    onInsert: (content: string) => void;
    getDocumentContent: () => string;
    onOpenSettings: () => void;
  }

  let { onApply, onInsert, getDocumentContent, onOpenSettings }: Props = $props();

  let input = $state("");
  let copiedIdx = $state<number | null>(null);
  let messagesEl: HTMLDivElement | undefined = $state();

  $effect(() => {
    void assistant.messages.length;
    void assistant.messages[assistant.messages.length - 1]?.content;
    void tick().then(() => messagesEl?.scrollTo({ top: messagesEl.scrollHeight }));
  });

  function handleSubmit(e: Event) {
    e.preventDefault();
    const text = input.trim();
    if (!text || assistant.isStreaming) return;
    void assistant.send(text, getDocumentContent());
    input = "";
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  }

  /** Largest fenced markdown block in a message, if any. */
  function extractBlock(content: string): string | null {
    const blocks: string[] = [];
    const regex = /```(?:markdown|md)?\n([\s\S]*?)```/g;
    let match;
    while ((match = regex.exec(content)) !== null) {
      blocks.push(match[1].trim());
    }
    if (blocks.length === 0) return null;
    return blocks.reduce((a, b) => (a.length > b.length ? a : b));
  }

  /** A block big enough to look like a full document rewrite. */
  function isFullRewrite(block: string): boolean {
    return block.length > 300 && /^#\s/m.test(block);
  }

  function copyMessage(content: string, idx: number) {
    void navigator.clipboard.writeText(content);
    copiedIdx = idx;
    setTimeout(() => (copiedIdx = null), 2000);
  }
</script>

{#if !assistant.isConfigured}
  <div class="assistant-empty">
    <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" opacity="0.3">
      <path d="M12 2a10 10 0 1 0 10 10 4 4 0 0 1-5-5 4 4 0 0 1-5-5" />
      <path d="M8.5 8.5v.01" />
      <path d="M16 15.5v.01" />
      <path d="M12 12v.01" />
      <path d="M11 17v.01" />
      <path d="M7 14v.01" />
    </svg>
    <p>Set up an AI provider to get started</p>
    <button class="assistant-settings-btn" onclick={onOpenSettings}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="3" />
        <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
      </svg>
      Settings
    </button>
  </div>
{:else}
  <div class="assistant-panel">
    <div class="assistant-header">
      <span class="assistant-header-title">Assistant</span>
      <div class="assistant-header-actions">
        {#if assistant.messages.length > 0}
          <button class="assistant-header-btn" onclick={() => assistant.clear()} title="Clear chat">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 6h18 M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6 M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
            </svg>
          </button>
        {/if}
        <button class="assistant-header-btn" onclick={onOpenSettings} title="AI settings">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3" />
            <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
          </svg>
        </button>
      </div>
    </div>

    <div class="assistant-messages" bind:this={messagesEl}>
      {#if assistant.messages.length === 0}
        <div class="assistant-welcome">
          <p>Ask me to review, improve, or generate content for your document.</p>
        </div>
      {/if}
      {#each assistant.messages as msg, i (i)}
        <div class="assistant-msg assistant-msg--{msg.role}">
          <div class="assistant-msg-content">{msg.content}</div>
          {#if msg.role === "assistant" && !assistant.isStreaming}
            {@const block = extractBlock(msg.content)}
            <div class="assistant-msg-actions">
              <button class="assistant-copy-btn" onclick={() => copyMessage(msg.content, i)}>
                {copiedIdx === i ? "Copied!" : "Copy"}
              </button>
              {#if block}
                <button class="assistant-apply-btn" onclick={() => onInsert(block)}>
                  Insert
                </button>
                {#if isFullRewrite(block)}
                  <button class="assistant-apply-btn" onclick={() => onApply(block)}>
                    Replace document
                  </button>
                {/if}
              {/if}
            </div>
          {/if}
        </div>
      {/each}
      {#if assistant.isStreaming}
        <div class="assistant-streaming">
          <span class="assistant-streaming-dot"></span>
        </div>
      {/if}
    </div>

    <form class="assistant-input-form" onsubmit={handleSubmit}>
      <textarea
        class="assistant-input"
        bind:value={input}
        onkeydown={handleKeyDown}
        placeholder="Ask about your document..."
        rows="2"
        disabled={assistant.isStreaming}
      ></textarea>
      <div class="assistant-input-actions">
        {#if assistant.isStreaming}
          <button type="button" class="assistant-send-btn" onclick={() => void assistant.stop()} title="Stop">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
              <rect x="6" y="6" width="12" height="12" rx="2" />
            </svg>
          </button>
        {:else}
          <button type="submit" class="assistant-send-btn" disabled={!input.trim()} title="Send">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="22" y1="2" x2="11" y2="13" />
              <polygon points="22 2 15 22 11 13 2 9 22 2" />
            </svg>
          </button>
        {/if}
      </div>
    </form>
  </div>
{/if}
