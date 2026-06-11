<script lang="ts">
  import { tick } from "svelte";
  import { assistant } from "$lib/assistant.svelte";
  import { toast } from "$lib/toast.svelte";
  import { api, type Document, type DocReference } from "$lib/api";
  import DocumentIcon from "$lib/components/DocumentIcon.svelte";

  interface Props {
    onApply: (content: string) => void;
    onInsert: (content: string) => void;
    getDocumentContent: () => string;
    documents: Document[];
    onOpenSettings: () => void;
  }

  let { onApply, onInsert, getDocumentContent, documents, onOpenSettings }: Props = $props();

  let input = $state("");
  let copiedIdx = $state<number | null>(null);
  let messagesEl: HTMLDivElement | undefined = $state();

  // @-mention: attach other documents as background context for one message.
  let inputEl: HTMLTextAreaElement | undefined = $state();
  /** The active `@token` query (text after `@`, before the caret), or null. */
  let mentionQuery = $state<string | null>(null);
  /** Docs chosen as references for the next message (capped, deduped). */
  let mentions = $state<Document[]>([]);
  const MAX_MENTIONS = 5;

  const mentionCandidates = $derived.by(() => {
    if (mentionQuery === null) return [];
    const q = mentionQuery.toLowerCase();
    return documents
      .filter((d) => d.type !== "idea" && !mentions.some((m) => m.id === d.id))
      .filter((d) => d.name.toLowerCase().includes(q))
      .slice(0, 6);
  });
  const showPicker = $derived(mentionQuery !== null && mentionCandidates.length > 0);

  /** Recompute the active @-token from the text before the caret. */
  function updateMentionQuery() {
    const caret = inputEl?.selectionStart ?? input.length;
    const m = input.slice(0, caret).match(/@([\w-]*)$/);
    mentionQuery = m ? m[1] : null;
  }

  function selectMention(doc: Document) {
    // strip the trailing @query fragment; the chip records the choice instead
    const caret = inputEl?.selectionStart ?? input.length;
    input = input.slice(0, caret).replace(/@([\w-]*)$/, "") + input.slice(caret);
    mentionQuery = null;
    if (mentions.length < MAX_MENTIONS && !mentions.some((m) => m.id === doc.id)) {
      mentions = [...mentions, doc];
    }
  }

  function removeMention(id: string) {
    mentions = mentions.filter((m) => m.id !== id);
  }

  /** Fetch each mentioned doc's body for the references payload. */
  async function buildReferences(): Promise<DocReference[]> {
    const refs: DocReference[] = [];
    for (const m of mentions) {
      try {
        refs.push({ name: m.name, content: await api.getDocumentContent(m.id) });
      } catch (e) {
        toast.error(`Couldn't attach "${m.name}": ${e}`);
      }
    }
    return refs;
  }

  /** Fire-and-forget chat op with a visible error toast on failure. */
  function guard(promise: Promise<unknown>, what: string) {
    promise.catch((e) => toast.error(`${what} failed: ${e}`));
  }

  /** Context size after the last completed turn (what the model last saw). */
  const contextTokens = $derived.by(() => {
    for (let i = assistant.messages.length - 1; i >= 0; i--) {
      const m = assistant.messages[i];
      if (m.role === "assistant" && m.inputTokens != null) {
        return m.inputTokens + (m.outputTokens ?? 0);
      }
    }
    return 0;
  });

  $effect(() => {
    void assistant.messages.length;
    void assistant.messages[assistant.messages.length - 1]?.content;
    void tick().then(() => messagesEl?.scrollTo({ top: messagesEl.scrollHeight }));
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    const text = input.trim();
    if (!text || assistant.isStreaming) return;
    const references = await buildReferences();
    void assistant.send(text, getDocumentContent(), references);
    input = "";
    mentions = [];
    mentionQuery = null;
  }

  function handleKeyDown(e: KeyboardEvent) {
    // while the @-picker is open, Enter picks the top match and Escape closes it
    if (showPicker && e.key === "Enter") {
      e.preventDefault();
      selectMention(mentionCandidates[0]);
      return;
    }
    if (mentionQuery !== null && e.key === "Escape") {
      e.preventDefault();
      mentionQuery = null;
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void handleSubmit(e);
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
      <select
        class="assistant-chat-select"
        value={assistant.activeChatId}
        onchange={(e) => guard(assistant.selectChat(e.currentTarget.value), "Switching chat")}
        title="Switch chat"
      >
        {#each assistant.chats as chat (chat.id)}
          <option value={chat.id}>{chat.title}</option>
        {/each}
      </select>
      <div class="assistant-header-actions">
        {#if contextTokens > 0}
          <span class="assistant-context" title="Context size after the last turn">
            ~{contextTokens.toLocaleString()} tok
          </span>
        {/if}
        <button class="assistant-header-btn" onclick={() => guard(assistant.newChat(), "New chat")} title="New chat">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
        {#if assistant.chats.length > 1}
          <button
            class="assistant-header-btn"
            onclick={() => guard(assistant.deleteChat(assistant.activeChatId!), "Delete chat")}
            title="Delete this chat"
          >
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
          {#if msg.role === "assistant" && msg.inputTokens != null}
            <div class="assistant-msg-usage">{msg.inputTokens} in · {msg.outputTokens} out</div>
          {/if}
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
      {#if showPicker}
        <div class="mention-picker">
          {#each mentionCandidates as doc (doc.id)}
            <button type="button" class="mention-option" onclick={() => selectMention(doc)}>
              <DocumentIcon type={doc.type} size={14} />
              <span class="mention-option-name">{doc.name}</span>
            </button>
          {/each}
        </div>
      {/if}
      {#if mentions.length}
        <div class="mention-chips">
          {#each mentions as m (m.id)}
            <span class="mention-chip">
              <DocumentIcon type={m.type} size={12} />
              <span class="mention-chip-name">{m.name}</span>
              <button type="button" class="mention-chip-remove" onclick={() => removeMention(m.id)} title="Remove">
                <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </span>
          {/each}
        </div>
      {/if}
      <textarea
        class="assistant-input"
        bind:value={input}
        bind:this={inputEl}
        oninput={updateMentionQuery}
        onkeydown={handleKeyDown}
        placeholder="Ask about your document… (@ to reference another)"
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

<style>
  .assistant-input-form {
    position: relative;
  }
  .mention-picker {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + 4px);
    max-height: 14rem;
    overflow-y: auto;
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.18);
    padding: 0.25rem;
    z-index: 20;
  }
  .mention-option {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    width: 100%;
    padding: 0.4rem 0.5rem;
    border: none;
    border-radius: calc(var(--radius) - 2px);
    background: transparent;
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
    font-size: 0.85rem;
  }
  .mention-option:hover {
    background: var(--bg-secondary);
  }
  .mention-option-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mention-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin-bottom: 0.4rem;
  }
  .mention-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.15rem 0.3rem 0.15rem 0.4rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-size: 0.75rem;
    max-width: 100%;
  }
  .mention-chip-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 10rem;
  }
  .mention-chip-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    flex-shrink: 0;
  }
  .mention-chip-remove:hover {
    color: var(--text-primary);
  }
</style>
