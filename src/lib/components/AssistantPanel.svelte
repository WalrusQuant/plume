<script lang="ts">
  import { tick } from "svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { assistant } from "$lib/assistant.svelte";
  import { aiBusy } from "$lib/aiBusy.svelte";
  import { toast } from "$lib/toast.svelte";
  import { formatError } from "$lib/formatError";
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
  /** True while the user is scrolled up reading history — pauses autoscroll. */
  let pinnedToBottom = $state(true);

  function onMessagesScroll() {
    if (!messagesEl) return;
    // Considered "at the bottom" if within ~80px (covers scrollbar + last msg padding)
    const dist = messagesEl.scrollHeight - messagesEl.scrollTop - messagesEl.clientHeight;
    pinnedToBottom = dist < 80;
  }

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
  async function buildReferences(docs: Document[]): Promise<DocReference[]> {
    const refs: DocReference[] = [];
    for (const m of docs) {
      try {
        refs.push({ name: m.name, content: await api.getDocumentContent(m.id) });
      } catch (e) {
        toast.error(`Couldn't attach "${m.name}": ${formatError(e)}`);
      }
    }
    return refs;
  }

  /** Fire-and-forget chat op with a visible error toast on failure. */
  function guard(promise: Promise<unknown>, what: string) {
    promise.catch((e) => toast.error(`${what} failed: ${formatError(e)}`));
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

  /** Within 85% of a hard context limit (OpenRouter) — warn before turns drop. */
  const nearLimit = $derived(
    assistant.contextLimit != null && contextTokens > 0.85 * assistant.contextLimit,
  );

  /** A user turn with no reply after it and nothing streaming — e.g. stopped
      before the first token. Marks it so the silence isn't unexplained. */
  const danglingUser = $derived(
    !assistant.isStreaming &&
      assistant.messages.length > 0 &&
      assistant.messages[assistant.messages.length - 1].role === "user",
  );

  $effect(() => {
    void assistant.messages.length;
    void assistant.messages[assistant.messages.length - 1]?.content;
    if (!pinnedToBottom) return; // user is reading history; don't yank them down
    void tick().then(() => messagesEl?.scrollTo({ top: messagesEl.scrollHeight }));
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    const text = input.trim();
    if (!text || assistant.isStreaming) return;
    // a headless generation owns the single AI slot; sending now would abort it
    // and discard its draft — block before consuming the input so it isn't lost
    if (aiBusy.busy) {
      toast.error(`Wait for the ${aiBusy.label} to finish before sending.`);
      return;
    }
    // consume the input synchronously — a second Enter during the awaited
    // reference fetch must be a no-op, not a silently dropped message
    const pendingMentions = mentions;
    input = "";
    mentions = [];
    mentionQuery = null;
    const references = await buildReferences(pendingMentions);
    const ok = await assistant.send(text, getDocumentContent(), references);
    if (!ok) {
      // send was rejected/failed — give the user their message back to retry
      input = text;
      mentions = pendingMentions;
    }
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

  async function copyMessage(content: string, idx: number) {
    try {
      await navigator.clipboard.writeText(content);
    } catch (e) {
      toast.error(`Couldn't copy: ${formatError(e)}`); // don't flash a false "Copied!"
      return;
    }
    copiedIdx = idx;
    setTimeout(() => (copiedIdx = null), 2000);
  }

  /** Toggle web search. Turning it on needs a Tavily key — if none is saved,
      explain and open Settings rather than enabling a no-op. */
  async function toggleSearch() {
    if (!assistant.settings.webSearch && !assistant.hasTavilyKey) {
      toast.error("Add a Tavily API key in Settings to turn on web search.");
      onOpenSettings();
      return;
    }
    await assistant.toggleWebSearch();
  }

  /** Toggle notes search — semantic search over the user's own docs. No key
      needed, so it just flips. */
  async function toggleNotes() {
    await assistant.toggleNotesSearch();
  }

  /** Confirm before irreversibly deleting the active chat thread. */
  async function handleDeleteChat() {
    if (!assistant.activeChatId) return;
    if (await confirm("Delete this chat? This can't be undone.", { kind: "warning" })) {
      guard(assistant.deleteChat(assistant.activeChatId), "Delete chat");
    }
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
          <span
            class="assistant-context"
            class:assistant-context--warn={nearLimit}
            title={nearLimit
              ? "Approaching the context limit — older messages will start dropping"
              : "Context size after the last turn"}
            aria-label={`${nearLimit ? "Approaching the context limit — " : "Context size: "}about ${contextTokens.toLocaleString()} tokens`}
          >
            ~{contextTokens.toLocaleString()} tokens
          </span>
        {/if}
        <button class="assistant-header-btn" onclick={() => guard(assistant.newChat(), "New chat")} title="New chat" aria-label="New chat">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
        {#if assistant.chats.length > 1}
          <button
            class="assistant-header-btn"
            onclick={() => void handleDeleteChat()}
            title="Delete this chat"
            aria-label="Delete this chat"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 6h18 M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6 M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
            </svg>
          </button>
        {/if}
        <button class="assistant-header-btn" onclick={onOpenSettings} title="AI settings" aria-label="AI settings">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3" />
            <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
          </svg>
        </button>
      </div>
    </div>

    {#if assistant.historyTrimmed}
      <div class="assistant-trim-note">
        Older messages were dropped to fit the context limit.
      </div>
    {/if}

    <div class="assistant-messages" bind:this={messagesEl} onscroll={onMessagesScroll}>
      {#if assistant.messages.length === 0}
        <div class="assistant-welcome">
          <p>Ask me to review, improve, or generate content for your document.</p>
        </div>
      {/if}
      {#each assistant.messages as msg, i (i)}
        <div class="assistant-msg assistant-msg--{msg.role}">
          <div class="assistant-msg-content">{msg.content}</div>
          {#if msg.role === "assistant" && msg.inputTokens != null}
            <div
              class="assistant-msg-usage"
              title="Tokens used — {msg.inputTokens} input, {msg.outputTokens} output"
              aria-label={`Tokens used — ${msg.inputTokens.toLocaleString()} input, ${(msg.outputTokens ?? 0).toLocaleString()} output`}
            >
              {(msg.inputTokens + (msg.outputTokens ?? 0)).toLocaleString()} tokens
            </div>
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
        <div class="assistant-streaming" role="status" aria-live="polite">
          {#if assistant.searchStatus}
            <svg class="assistant-search-spin" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10" />
              <path d="M2 12h20" />
              <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
            </svg>
            <span class="assistant-search-status">{assistant.searchStatus}</span>
          {:else}
            <span class="assistant-streaming-dot" aria-hidden="true"></span>
            <span class="sr-only">Generating…</span>
          {/if}
        </div>
      {/if}
      {#if danglingUser}
        <div class="assistant-no-response">No response — the reply was stopped before it started.</div>
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
        <button
          type="button"
          class="assistant-search-toggle"
          class:assistant-search-toggle--active={assistant.settings.searchNotes}
          onclick={toggleNotes}
          aria-pressed={assistant.settings.searchNotes}
          title={assistant.settings.searchNotes ? "Notes search on — click to turn off" : "Notes search off — click to turn on (searches your own docs)"}
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
            <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
          </svg>
        </button>
        <button
          type="button"
          class="assistant-search-toggle"
          class:assistant-search-toggle--active={assistant.settings.webSearch}
          onclick={toggleSearch}
          aria-pressed={assistant.settings.webSearch}
          title={assistant.settings.webSearch ? "Web search on — click to turn off" : "Web search off — click to turn on"}
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10" />
            <path d="M2 12h20" />
            <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
          </svg>
        </button>
        {#if assistant.isStreaming}
          <button type="button" class="assistant-send-btn" onclick={() => void assistant.stop()} title="Stop" aria-label="Stop">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
              <rect x="6" y="6" width="12" height="12" rx="2" />
            </svg>
          </button>
        {:else}
          <button type="submit" class="assistant-send-btn" disabled={!input.trim()} title="Send" aria-label="Send">
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
  .assistant-context--warn {
    color: var(--error, #e5484d);
  }
  .assistant-trim-note {
    padding: 6px 12px;
    font-size: 11.5px;
    color: var(--text-tertiary);
    border-bottom: 1px solid var(--border);
    background: var(--bg-secondary);
  }
  .assistant-no-response {
    padding: 4px 12px 8px;
    font-size: 11.5px;
    font-style: italic;
    color: var(--text-tertiary);
  }
</style>
