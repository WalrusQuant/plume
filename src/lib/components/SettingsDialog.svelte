<script lang="ts">
  import { api, type AIProvider, type ModelStatus, type EmbedModelInfo } from "$lib/api";
  import { assistant, DEFAULT_MODELS } from "$lib/assistant.svelte";
  import { toast } from "$lib/toast.svelte";
  import Dialog from "$lib/components/Dialog.svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  const MODEL_SUGGESTIONS: Record<AIProvider, string[]> = {
    anthropic: ["claude-opus-4-8", "claude-sonnet-4-6", "claude-haiku-4-5"],
    openrouter: [
      "anthropic/claude-opus-4.8",
      "anthropic/claude-sonnet-4.6",
      "anthropic/claude-haiku-4.5",
    ],
  };

  let formProvider = $state<AIProvider>("anthropic");
  let formModel = $state("");
  let formVoice = $state("");
  let keyInput = $state("");
  let keyError = $state("");
  let tavilyKeyError = $state("");
  let hasSavedKey = $state(false);
  let tavilyKeyInput = $state("");
  let hasSavedTavilyKey = $state(false);
  let activeTab = $state<"ai" | "local">("ai");
  let modelStatus = $state<ModelStatus | null>(null);
  let models = $state<EmbedModelInfo[]>([]);
  let modelBusy = $state<"" | "downloading" | "removing" | "switching">("");

  // The catalog entry for the currently-active model (for its size label + note).
  const activeModel = $derived(
    models.find((m) => m.id === modelStatus?.activeModelId) ?? null,
  );

  function formatSize(bytes: number): string {
    return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  }

  // re-seed the form from saved settings each time the dialog opens
  $effect(() => {
    if (open) {
      activeTab = "ai";
      formProvider = assistant.settings.provider;
      formModel = assistant.settings.model;
      formVoice = assistant.settings.voice;
      keyInput = "";
      tavilyKeyInput = "";
      keyError = "";
      tavilyKeyError = "";
      modelBusy = "";
      void api.hasTavilyKey().then((has) => (hasSavedTavilyKey = has));
      void refreshModels();
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
    const provider = formProvider;
    void api.hasApiKey(provider).then((has) => {
      if (provider === formProvider) hasSavedKey = has;
    });
  });

  function onProviderChange(provider: AIProvider) {
    formProvider = provider;
    formModel = DEFAULT_MODELS[provider];
    // Drop any key typed for the previous provider so it can't be saved under
    // the newly selected one. The status line re-checks for the new provider.
    keyInput = "";
    keyError = "";
  }

  async function save(e: Event) {
    e.preventDefault();
    keyError = "";
    tavilyKeyError = "";
    // Snapshot the inputs BEFORE any await: updateSettings() mutates
    // assistant.settings, which re-runs the re-seed $effect and would clear the
    // bound key fields mid-save (the writes below would then see empty strings).
    const provider = formProvider;
    // An empty model would break the next API call — fall back to the default.
    const model = formModel.trim() || DEFAULT_MODELS[provider];
    const voice = formVoice.trim();
    const newKey = keyInput.trim();
    const newTavilyKey = tavilyKeyInput.trim();
    try {
      // settings first so saveKey() stores under the (possibly changed) provider
      await assistant.updateSettings({
        ...assistant.settings,
        provider,
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

  /** Remove the stored key for the provider currently shown in the form. */
  async function removeKey() {
    try {
      if (formProvider === assistant.settings.provider) {
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

<Dialog {open} title="Settings" {onClose}>
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
    </div>

    {#if activeTab === "ai"}
      <form id="settings-form" onsubmit={save}>
        <label class="dialog-label" for="settings-provider">AI provider</label>
        <div class="assistant-provider-row" id="settings-provider">
          <button
            type="button"
            class="dialog-type-card {formProvider === 'anthropic' ? 'dialog-type-card--active' : ''}"
            onclick={() => onProviderChange("anthropic")}
          >
            <span class="dialog-type-label">Anthropic</span>
          </button>
          <button
            type="button"
            class="dialog-type-card {formProvider === 'openrouter' ? 'dialog-type-card--active' : ''}"
            onclick={() => onProviderChange("openrouter")}
          >
            <span class="dialog-type-label">OpenRouter</span>
          </button>
        </div>

        <label class="dialog-label" for="settings-model">Model</label>
        <input
          id="settings-model"
          class="dialog-input"
          type="text"
          list="model-suggestions"
          placeholder={DEFAULT_MODELS[formProvider]}
          bind:value={formModel}
          autocomplete="off"
        />
        <datalist id="model-suggestions">
          {#each MODEL_SUGGESTIONS[formProvider] as model (model)}
            <option value={model}></option>
          {/each}
        </datalist>

        <label class="dialog-label" for="settings-voice">Voice &amp; tone</label>
        <textarea
          id="settings-voice"
          class="dialog-textarea"
          rows="4"
          placeholder="Describe how the AI should write for you — tone, rhythm, words you love or avoid. Applies to chat, inline edits, and idea expansion. Leave blank for neutral."
          bind:value={formVoice}
        ></textarea>
        <p class="assistant-key-note">
          Your voice is added to every AI request so generated text sounds like you.
        </p>

        <label class="dialog-label" for="settings-key">API key</label>
        <input
          id="settings-key"
          class="dialog-input"
          type="password"
          placeholder={hasSavedKey
            ? "Key saved — leave blank to keep it"
            : formProvider === "anthropic"
              ? "sk-ant-..."
              : "sk-or-..."}
          bind:value={keyInput}
          autocomplete="off"
        />
        <p class="assistant-key-status">
          {hasSavedKey
            ? "✓ A key is saved for this provider. Fill this in only to replace it."
            : "No key saved for this provider yet."}
          {#if hasSavedKey}
            <button type="button" class="key-remove-btn" onclick={removeKey}>Remove</button>
          {/if}
        </p>
        {#if keyError}
          <p class="assistant-key-error">{keyError}</p>
        {/if}
        <p class="assistant-key-note">
          {import.meta.env.DEV
            ? "Dev build: keys are stored in a local file in the app data folder (keychain is skipped to avoid password prompts)."
            : "Keys are stored in the macOS Keychain — they never leave this machine except to call your AI provider."}
        </p>

        <label class="dialog-label" for="settings-tavily-key">Web search key (Tavily)</label>
        <input
          id="settings-tavily-key"
          class="dialog-input"
          type="password"
          placeholder={hasSavedTavilyKey ? "Key saved — leave blank to keep it" : "tvly-..."}
          bind:value={tavilyKeyInput}
          autocomplete="off"
        />
        <p class="assistant-key-status">
          {hasSavedTavilyKey
            ? "✓ A Tavily key is saved. Fill this in only to replace it."
            : "No Tavily key saved. Add one to let the assistant search the web."}
          {#if hasSavedTavilyKey}
            <button type="button" class="key-remove-btn" onclick={removeTavilyKey}>Remove</button>
          {/if}
        </p>
        <p class="assistant-key-note">
          The assistant searches the web with Tavily when you toggle search on in the chat.
          Get a free key at app.tavily.com (1,000 searches/month).
        </p>
        {#if tavilyKeyError}
          <p class="assistant-key-error">{tavilyKeyError}</p>
        {/if}
      </form>
    {:else}
      <div class="local-search-tab">
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
          <p class="assistant-key-note">{activeModel.note}</p>
        {/if}

        {#if modelStatus?.installed}
          <p class="assistant-key-status">
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
          <p class="assistant-key-note model-path">{modelStatus.path}</p>
        {:else}
          <p class="assistant-key-status">
            {modelBusy === "switching" ? "Switching…" : "Not installed."}
          </p>
          <button
            type="button"
            class="dialog-btn dialog-btn--secondary model-download-btn"
            disabled={modelBusy !== ""}
            onclick={downloadModel}
          >
            {modelBusy === "downloading"
              ? "Downloading…"
              : `Download model (${activeModel?.sizeLabel ?? ""})`}
          </button>
        {/if}

        <p class="assistant-key-note">
          Powers “search your notes” in chat — the model runs entirely on your machine, so
          your documents never leave it. Changing the model re-indexes all your notes with
          the new one; removing it frees the disk (already-indexed notes still search, only
          new edits pause until a model is installed again).
        </p>
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

  .model-download-btn {
    margin-top: 6px;
  }
  .model-download-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .model-path {
    font-family: var(--font-mono, ui-monospace, monospace);
    word-break: break-all;
    user-select: text;
  }

  .settings-tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 16px;
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
