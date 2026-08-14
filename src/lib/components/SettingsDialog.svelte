<script lang="ts">
  import { api, type AIProvider, type CustomConnector, type ModelStatus, type EmbedModelInfo } from "$lib/api";
  import { assistant, DEFAULT_MODELS } from "$lib/assistant.svelte";
  import { isBuiltinProvider, type BuiltinProvider } from "$lib/aiSettings";
  import { toast } from "$lib/toast.svelte";
  import { formatError } from "$lib/formatError";
  import Dialog from "$lib/components/Dialog.svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  const BUILTIN_CARDS: { id: BuiltinProvider; label: string }[] = [
    { id: "anthropic", label: "Anthropic" },
    { id: "openai", label: "OpenAI" },
    { id: "grok", label: "Grok" },
    { id: "openrouter", label: "OpenRouter" },
  ];

  const MODEL_SUGGESTIONS: Record<BuiltinProvider, string[]> = {
    anthropic: ["claude-opus-4-8", "claude-sonnet-4-6", "claude-haiku-4-5"],
    openai: ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"],
    grok: ["grok-4.6", "grok-4.5", "grok-4.3", "grok-build-0.1"],
    openrouter: [
      "anthropic/claude-opus-4.8",
      "anthropic/claude-sonnet-4.6",
      "anthropic/claude-haiku-4.5",
    ],
  };

  let formProvider = $state<AIProvider>("anthropic");
  let formCustomId = $state<string | null>(null);
  let formConnectors = $state<CustomConnector[]>([]);
  let removedCustomIds = $state<string[]>([]);
  let formModel = $state("");
  let formVoice = $state("");
  let keyInput = $state("");
  let keyError = $state("");
  let tavilyKeyError = $state("");
  let hasSavedKey = $state(false);
  let tavilyKeyInput = $state("");
  let hasSavedTavilyKey = $state(false);
  let activeTab = $state<"ai" | "local" | "agents">("ai");
  let mcpStatus = $state<{ binaryPath: string; installed: boolean; buildCommand: string } | null>(
    null,
  );
  let mcpCopied = $state("");
  let modelStatus = $state<ModelStatus | null>(null);
  let models = $state<EmbedModelInfo[]>([]);
  let modelBusy = $state<"" | "downloading" | "removing" | "switching">("");
  let testStatus = $state<"" | "checking" | "ok" | "err">("");
  let testMessage = $state("");

  // The catalog entry for the currently-active model (for its size label + note).
  const activeModel = $derived(
    models.find((m) => m.id === modelStatus?.activeModelId) ?? null,
  );
  const selectedCustom = $derived(
    formConnectors.find((c) => c.id === formCustomId) ?? null,
  );
  const modelSuggestions = $derived(
    isBuiltinProvider(formProvider) ? MODEL_SUGGESTIONS[formProvider] : [],
  );

  function formatSize(bytes: number): string {
    return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  }

  // re-seed the form from saved settings each time the dialog opens
  $effect(() => {
    if (open) {
      activeTab = "ai";
      formProvider = assistant.settings.provider;
      formCustomId = assistant.settings.customId;
      formConnectors = assistant.settings.customConnectors.map((c) => ({ ...c }));
      removedCustomIds = [];
      formModel = assistant.settings.model;
      formVoice = assistant.settings.voice;
      keyInput = "";
      tavilyKeyInput = "";
      keyError = "";
      tavilyKeyError = "";
      modelBusy = "";
      mcpCopied = "";
      testStatus = "";
      testMessage = "";
      void api.hasTavilyKey().then((has) => (hasSavedTavilyKey = has));
      void refreshModels();
      void api
        .mcpStatus()
        .then((s) => (mcpStatus = s))
        .catch((err) => toast.error(String(err)));
    }
  });

  /** Reload the active model's status + the catalog (installed flags). */
  async function refreshModels() {
    try {
      const [status, list] = await Promise.all([
        api.embedModelStatus(),
        api.listEmbedModels(),
      ]);
      modelStatus = status;
      models = list;
    } catch (err) {
      toast.error(String(err));
    }
  }

  // Re-check on every open (not just on provider change): otherwise reopening
  // the dialog with the same provider keeps a stale hasSavedKey from before the
  // key was saved, and the status line wrongly reads "No key saved".
  $effect(() => {
    if (!open) return;
    if (formProvider === "custom") {
      const id = formCustomId;
      if (!id) {
        hasSavedKey = false;
        return;
      }
      void api.hasCustomApiKey(id).then((has) => {
        if (formProvider === "custom" && formCustomId === id) hasSavedKey = has;
      });
      return;
    }
    const provider = formProvider;
    void api.hasApiKey(provider).then((has) => {
      if (provider === formProvider) hasSavedKey = has;
    });
  });

  type McpClient = "cursor" | "claude" | "grok" | "codex" | "vscode";

  function mcpSnippet(kind: McpClient): string {
    const cmd = mcpStatus?.binaryPath ?? "/path/to/plume-mcp";
    if (kind === "vscode") {
      return JSON.stringify(
        { servers: { plume: { type: "stdio", command: cmd } } },
        null,
        2,
      );
    }
    if (kind === "grok") {
      return `grok mcp add plume -- ${cmd}`;
    }
    if (kind === "codex") {
      return `codex mcp add plume -- ${cmd}`;
    }
    return JSON.stringify(
      { mcpServers: { plume: { command: cmd } } },
      null,
      2,
    );
  }

  async function copyMcp(kind: McpClient) {
    try {
      await navigator.clipboard.writeText(mcpSnippet(kind));
      mcpCopied = kind;
    } catch (err) {
      toast.error(String(err));
    }
  }

  function fallbackModel(provider: AIProvider, customId: string | null): string {
    if (provider === "custom") {
      return formConnectors.find((c) => c.id === customId)?.model ?? "";
    }
    return DEFAULT_MODELS[provider];
  }

  function keyPlaceholder(): string {
    if (hasSavedKey) return "Key saved — leave blank to keep it";
    if (formProvider === "anthropic") return "sk-ant-...";
    if (formProvider === "openai") return "sk-...";
    if (formProvider === "grok") return "xai-...";
    if (formProvider === "openrouter") return "sk-or-...";
    return "Optional — leave blank for local servers";
  }

  function rememberCustomModel() {
    if (formProvider !== "custom" || !formCustomId) return;
    const id = formCustomId;
    const model = formModel;
    formConnectors = formConnectors.map((c) => (c.id === id ? { ...c, model } : c));
  }

  function onProviderChange(provider: BuiltinProvider) {
    rememberCustomModel();
    formProvider = provider;
    formCustomId = null;
    formModel = DEFAULT_MODELS[provider];
    keyInput = "";
    keyError = "";
    testStatus = "";
    testMessage = "";
  }

  function onSelectCustom(id: string) {
    rememberCustomModel();
    formProvider = "custom";
    formCustomId = id;
    formModel = formConnectors.find((c) => c.id === id)?.model ?? "";
    keyInput = "";
    keyError = "";
    testStatus = "";
    testMessage = "";
  }

  function addCustom() {
    const id = crypto.randomUUID();
    formConnectors = [
      ...formConnectors,
      { id, name: "New endpoint", baseUrl: "", model: "" },
    ];
    onSelectCustom(id);
  }

  function removeCustom(id: string) {
    formConnectors = formConnectors.filter((c) => c.id !== id);
    if (!removedCustomIds.includes(id)) removedCustomIds = [...removedCustomIds, id];
    if (formCustomId === id) {
      formProvider = "anthropic";
      formCustomId = null;
      formModel = DEFAULT_MODELS.anthropic;
      keyInput = "";
    }
  }

  async function save(e: Event) {
    e.preventDefault();
    keyError = "";
    tavilyKeyError = "";
    // Snapshot the inputs BEFORE any await: updateSettings() mutates
    // assistant.settings, which re-runs the re-seed $effect and would clear the
    // bound key fields mid-save (the writes below would then see empty strings).
    const provider = formProvider;
    const customId = formCustomId;
    const connectors = formConnectors.map((c) =>
      c.id === customId ? { ...c, model: formModel.trim() } : { ...c },
    );
    const model = formModel.trim() || fallbackModel(provider, customId);
    const voice = formVoice.trim();
    const newKey = keyInput.trim();
    const newTavilyKey = tavilyKeyInput.trim();
    const removedIds = [...removedCustomIds];
    try {
      for (const id of removedIds) {
        await api.deleteCustomApiKey(id).catch(() => {});
      }
      // settings first so saveKey() stores under the (possibly changed) provider
      await assistant.updateSettings({
        ...assistant.settings,
        provider,
        customId,
        customConnectors: connectors,
        model,
        voice,
      });
      if (newKey) {
        await assistant.saveKey(newKey);
      }
      if (newTavilyKey) {
        await assistant.saveTavilyKey(newTavilyKey);
      }
      // Saving settings/keys always succeeds on its own — a missing provider key
      // only means the assistant can't chat yet, which the status line already
      // states. Never block the save or show an error for it.
      keyInput = "";
      tavilyKeyInput = "";
      toast.show("Settings saved", "info");
      onClose();
    } catch (err) {
      keyError = String(err);
    }
  }

  async function testConnection() {
    testStatus = "checking";
    testMessage = "";
    const connector =
      formProvider === "custom"
        ? {
            provider: "custom" as const,
            customId: formCustomId,
            baseUrl: selectedCustom?.baseUrl ?? "",
          }
        : { provider: formProvider };
    try {
      testMessage = await api.testConnector(
        connector,
        formModel.trim() || fallbackModel(formProvider, formCustomId) || null,
        keyInput.trim() || null,
      );
      testStatus = "ok";
    } catch (err) {
      testStatus = "err";
      testMessage = formatError(err);
    }
  }

  /** Remove the stored key for the provider currently shown in the form. */
  async function removeKey() {
    try {
      if (formProvider === "custom") {
        if (formCustomId) await api.deleteCustomApiKey(formCustomId);
      } else if (formProvider === assistant.settings.provider) {
        await assistant.removeKey();
      } else {
        await api.deleteApiKey(formProvider);
      }
      hasSavedKey = false;
      keyInput = "";
      toast.show("API key removed", "info");
    } catch (err) {
      keyError = String(err);
    }
  }

  async function removeTavilyKey() {
    try {
      await assistant.removeTavilyKey();
      hasSavedTavilyKey = false;
      tavilyKeyInput = "";
      toast.show("Tavily key removed", "info");
    } catch (err) {
      tavilyKeyError = String(err);
    }
  }

  async function downloadModel() {
    modelBusy = "downloading";
    try {
      modelStatus = await api.downloadEmbedModel();
      models = await api.listEmbedModels();
      toast.show("Search model downloaded — your notes will index now.", "info");
    } catch (err) {
      toast.error(String(err));
    } finally {
      modelBusy = "";
    }
  }

  async function removeModel() {
    modelBusy = "removing";
    try {
      modelStatus = await api.removeEmbedModel();
      models = await api.listEmbedModels();
      toast.show("Search model removed", "info");
    } catch (err) {
      toast.error(String(err));
    } finally {
      modelBusy = "";
    }
  }

  /** Switch the active model. The backend wipes + re-indexes; a switch to the
      already-active model is a no-op there, so guard it here too. */
  async function switchModel(id: string) {
    if (modelBusy !== "" || !modelStatus || id === modelStatus.activeModelId) return;
    modelBusy = "switching";
    try {
      modelStatus = await api.setEmbedModel(id);
      models = await api.listEmbedModels();
      toast.show(
        modelStatus.installed
          ? "Search model changed — re-indexing your notes."
          : "Search model changed — download it below to re-index your notes.",
        "info",
      );
    } catch (err) {
      toast.error(String(err));
      // Re-sync so the dropdown snaps back to the still-active model on failure.
      await refreshModels();
    } finally {
      modelBusy = "";
    }
  }
</script>

<Dialog {open} title="Settings" {onClose} wide>
  <div class="dialog-body">
    <div class="settings-tabs" role="tablist">
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "ai"}
        class="settings-tab {activeTab === 'ai' ? 'settings-tab--active' : ''}"
        onclick={() => (activeTab = "ai")}
      >
        AI
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "local"}
        class="settings-tab {activeTab === 'local' ? 'settings-tab--active' : ''}"
        onclick={() => (activeTab = "local")}
      >
        Local search
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "agents"}
        class="settings-tab {activeTab === 'agents' ? 'settings-tab--active' : ''}"
        onclick={() => (activeTab = "agents")}
      >
        Agents
      </button>
    </div>

    {#if activeTab === "ai"}
      <form id="settings-form" class="settings-stack" onsubmit={save}>
        <div class="settings-field">
          <label class="dialog-label" for="settings-provider">Connector</label>
          <div class="assistant-provider-row settings-provider-grid" id="settings-provider">
            {#each BUILTIN_CARDS as card (card.id)}
              <button
                type="button"
                class="dialog-type-card {formProvider === card.id ? 'dialog-type-card--active' : ''}"
                onclick={() => onProviderChange(card.id)}
              >
                <span class="dialog-type-label">{card.label}</span>
              </button>
            {/each}
          </div>
        </div>

        <div class="settings-field">
          <p class="dialog-label" id="settings-custom-label">Your endpoints</p>
          <div class="settings-custom-row" role="group" aria-labelledby="settings-custom-label">
            {#each formConnectors as connector (connector.id)}
              <button
                type="button"
                class="dialog-type-card {formProvider === 'custom' && formCustomId === connector.id
                  ? 'dialog-type-card--active'
                  : ''}"
                onclick={() => onSelectCustom(connector.id)}
              >
                <span class="dialog-type-label">{connector.name || "Untitled"}</span>
              </button>
            {/each}
            <button type="button" class="dialog-type-card settings-add-endpoint" onclick={addCustom}>
              <span class="dialog-type-label">+ Add endpoint</span>
            </button>
          </div>
          <p class="settings-help">
            Custom endpoints speak the OpenAI chat-completions API. Point them at
            Ollama, Together, a proxy, or any compatible server.
          </p>
        </div>

        {#if formProvider === "custom" && selectedCustom}
          <div class="settings-field">
            <label class="dialog-label" for="settings-custom-name">Endpoint name</label>
            <input
              id="settings-custom-name"
              class="dialog-input"
              type="text"
              bind:value={selectedCustom.name}
              placeholder="Ollama"
              autocomplete="off"
            />
          </div>
          <div class="settings-field">
            <label class="dialog-label" for="settings-custom-url">Base URL</label>
            <input
              id="settings-custom-url"
              class="dialog-input"
              type="url"
              bind:value={selectedCustom.baseUrl}
              placeholder="http://localhost:11434/v1"
              autocomplete="off"
            />
            <p class="settings-help">
              Use the server’s <code>/v1</code> root. <code>/chat/completions</code> is added if
              it is missing.
            </p>
            <button
              type="button"
              class="key-remove-btn"
              onclick={() => selectedCustom && removeCustom(selectedCustom.id)}
            >
              Remove this endpoint
            </button>
          </div>
        {/if}

        <div class="settings-field">
          <label class="dialog-label" for="settings-model">Model</label>
          <input
            id="settings-model"
            class="dialog-input"
            type="text"
            list="model-suggestions"
            placeholder={fallbackModel(formProvider, formCustomId) || "model-id"}
            bind:value={formModel}
            autocomplete="off"
          />
          <datalist id="model-suggestions">
            {#each modelSuggestions as model (model)}
              <option value={model}></option>
            {/each}
          </datalist>
        </div>

        <div class="settings-field">
          <label class="dialog-label" for="settings-voice">Voice &amp; tone</label>
          <textarea
            id="settings-voice"
            class="dialog-textarea"
            rows="4"
            placeholder="Describe how the AI should write for you — tone, rhythm, words you love or avoid. Applies to chat, inline edits, and idea expansion. Leave blank for neutral."
            bind:value={formVoice}
          ></textarea>
          <p class="settings-help">
            Your voice is added to every AI request so generated text sounds like you.
          </p>
        </div>

        <div class="settings-field">
          <label class="dialog-label" for="settings-key">API key</label>
          <input
            id="settings-key"
            class="dialog-input"
            type="password"
            placeholder={keyPlaceholder()}
            bind:value={keyInput}
            autocomplete="off"
          />
          <div class="settings-meta">
            <p class="settings-status">
              {hasSavedKey
                ? "✓ A key is saved for this provider. Fill this in only to replace it."
                : "No key saved for this provider yet."}
              {#if hasSavedKey}
                <button type="button" class="key-remove-btn" onclick={removeKey}>Remove</button>
              {/if}
            </p>
            {#if keyError}
              <p class="settings-error">{keyError}</p>
            {/if}
            <p class="settings-help">
              {#if formProvider === "custom"}
                Optional for local servers that do not require a key.
              {:else}
                Keys are stored in a locked file on this machine. They never
                leave except to call your AI provider.
              {/if}
            </p>
            <div class="settings-test-row">
              <button
                type="button"
                class="dialog-btn dialog-btn--secondary"
                disabled={testStatus === "checking"}
                onclick={() => void testConnection()}
              >
                {testStatus === "checking" ? "Testing…" : "Test connection"}
              </button>
              {#if testStatus === "ok"}
                <p class="settings-status">{testMessage}</p>
              {:else if testStatus === "err"}
                <p class="settings-error">{testMessage}</p>
              {/if}
            </div>
          </div>
        </div>

        <div class="settings-field">
          <label class="dialog-label" for="settings-tavily-key">Web search key (Tavily)</label>
          <input
            id="settings-tavily-key"
            class="dialog-input"
            type="password"
            placeholder={hasSavedTavilyKey ? "Key saved — leave blank to keep it" : "tvly-..."}
            bind:value={tavilyKeyInput}
            autocomplete="off"
          />
          <div class="settings-meta">
            <p class="settings-status">
              {hasSavedTavilyKey
                ? "✓ A Tavily key is saved. Fill this in only to replace it."
                : "No Tavily key saved. Add one to let the assistant search the web."}
              {#if hasSavedTavilyKey}
                <button type="button" class="key-remove-btn" onclick={removeTavilyKey}>Remove</button>
              {/if}
            </p>
            {#if tavilyKeyError}
              <p class="settings-error">{tavilyKeyError}</p>
            {/if}
            <p class="settings-help">
              The assistant searches the web with Tavily when you toggle search on in the chat.
              Get a free key at app.tavily.com (1,000 searches/month).
            </p>
          </div>
        </div>
      </form>
    {:else if activeTab === "local"}
      <div class="settings-stack">
        <div class="settings-field">
          <label class="dialog-label" for="embed-model-select">Search model</label>
          <select
            id="embed-model-select"
            class="dialog-input"
            value={modelStatus?.activeModelId ?? ""}
            disabled={modelBusy !== ""}
            onchange={(e) => switchModel(e.currentTarget.value)}
          >
            {#each models as m (m.id)}
              <option value={m.id}>
                {m.label} · {m.dim}d · {m.sizeLabel}{m.installed ? " · installed" : ""}
              </option>
            {/each}
          </select>
          {#if activeModel}
            <p class="settings-help">{activeModel.note}</p>
          {/if}
        </div>

        <div class="settings-field">
          {#if modelStatus?.installed}
            <p class="settings-status">
              ✓ Installed ({formatSize(modelStatus.sizeBytes)}).
              <button
                type="button"
                class="key-remove-btn"
                disabled={modelBusy !== ""}
                onclick={removeModel}
              >
                {modelBusy === "removing" ? "Removing…" : "Remove"}
              </button>
            </p>
            <p class="settings-path">{modelStatus.path}</p>
          {:else}
            <p class="settings-status">
              {modelBusy === "switching" ? "Switching…" : "Not installed."}
            </p>
            <button
              type="button"
              class="dialog-btn dialog-btn--secondary"
              disabled={modelBusy !== ""}
              onclick={downloadModel}
            >
              {modelBusy === "downloading"
                ? "Downloading…"
                : `Download model (${activeModel?.sizeLabel ?? ""})`}
            </button>
          {/if}
        </div>

        <div class="settings-meta">
          <p class="settings-help">
            Powers “search your notes” in chat. The model runs entirely on your machine, so
            your documents never leave it.
          </p>
          <p class="settings-help">
            Changing the model re-indexes all your notes. Removing it frees the disk —
            already-indexed notes still search, but new edits pause until a model is
            installed again.
          </p>
        </div>
      </div>
    {:else}
      <div class="settings-stack">
        <p class="settings-help">
          Connect a coding agent to this notebook. It can read and update project
          plans even when Plume is closed. No token — the agent launches the
          local <code>plume-mcp</code> binary over stdio.
        </p>
        {#if mcpStatus}
          <div class="settings-field">
            <p class="settings-status">
              {mcpStatus.installed ? "✓ Agent server is built." : "Agent server not built yet."}
            </p>
            <p class="settings-path">{mcpStatus.binaryPath}</p>
            {#if !mcpStatus.installed}
              <p class="settings-help">
                Build it once from the repo:
                <code>{mcpStatus.buildCommand}</code>
              </p>
            {/if}
          </div>
        {/if}
        <div class="settings-field">
          <p class="dialog-label">Copy a config</p>
          <div class="mcp-copy-row">
            <button type="button" class="dialog-btn dialog-btn--secondary" onclick={() => copyMcp("cursor")}>
              {mcpCopied === "cursor" ? "Copied" : "Cursor"}
            </button>
            <button type="button" class="dialog-btn dialog-btn--secondary" onclick={() => copyMcp("claude")}>
              {mcpCopied === "claude" ? "Copied" : "Claude Code"}
            </button>
            <button type="button" class="dialog-btn dialog-btn--secondary" onclick={() => copyMcp("grok")}>
              {mcpCopied === "grok" ? "Copied" : "Grok"}
            </button>
            <button type="button" class="dialog-btn dialog-btn--secondary" onclick={() => copyMcp("codex")}>
              {mcpCopied === "codex" ? "Copied" : "Codex"}
            </button>
            <button type="button" class="dialog-btn dialog-btn--secondary" onclick={() => copyMcp("vscode")}>
              {mcpCopied === "vscode" ? "Copied" : "VS Code"}
            </button>
          </div>
          <p class="settings-help">
            Cursor / Claude Code: paste into MCP settings. Grok and Codex: run the
            copied command. VS Code: paste into <code>.vscode/mcp.json</code>.
          </p>
        </div>
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <div class="dialog-footer">
      <button type="button" class="dialog-btn dialog-btn--secondary" onclick={onClose}>
        {activeTab === "ai" ? "Cancel" : "Close"}
      </button>
      {#if activeTab === "ai"}
        <button type="submit" form="settings-form" class="dialog-btn dialog-btn--primary">Save</button>
      {/if}
    </div>
  {/snippet}
</Dialog>

<style>
  .settings-stack {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .settings-provider-grid {
    flex-wrap: wrap;
  }

  .settings-provider-grid :global(.dialog-type-card) {
    flex: 1 1 calc(50% - 4px);
  }

  .settings-custom-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .settings-add-endpoint {
    border-style: dashed;
  }

  .settings-test-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  .settings-field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .settings-field .dialog-label {
    margin-bottom: 0;
  }

  .settings-field .dialog-textarea {
    margin-top: 0;
    min-height: 104px;
  }

  .settings-meta {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .settings-help,
  .settings-status,
  .settings-error,
  .settings-path {
    margin: 0;
    max-width: none;
    font-size: 12.5px;
    line-height: 1.55;
  }

  .settings-help {
    color: var(--text-tertiary);
  }

  .settings-status {
    color: var(--text-secondary);
  }

  .settings-error {
    color: var(--danger);
  }

  .settings-path {
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--text-tertiary);
    word-break: break-all;
    user-select: text;
  }

  .settings-help code,
  .settings-path {
    font-size: 11.5px;
  }

  .key-remove-btn {
    margin-left: 6px;
    padding: 0;
    border: none;
    background: none;
    color: var(--error, #e5484d);
    font-size: inherit;
    cursor: pointer;
    text-decoration: underline;
  }
  .key-remove-btn:disabled {
    opacity: 0.6;
    cursor: default;
    text-decoration: none;
  }

  .mcp-copy-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .settings-field .dialog-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .settings-tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 20px;
    border-bottom: 1px solid var(--border);
  }
  .settings-tab {
    padding: 8px 14px;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    background: none;
    color: var(--text-secondary);
    font-size: inherit;
    font-weight: 500;
    cursor: pointer;
  }
  .settings-tab:hover {
    color: var(--text-primary);
  }
  .settings-tab--active {
    color: var(--text-primary);
    border-bottom-color: var(--accent);
  }
</style>
