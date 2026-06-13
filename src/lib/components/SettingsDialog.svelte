<script lang="ts">
  import { api, type AIProvider } from "$lib/api";
  import { assistant, DEFAULT_MODELS } from "$lib/assistant.svelte";

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
  let hasSavedKey = $state(false);
  let tavilyKeyInput = $state("");
  let hasSavedTavilyKey = $state(false);

  // re-seed the form from saved settings each time the dialog opens
  $effect(() => {
    if (open) {
      formProvider = assistant.settings.provider;
      formModel = assistant.settings.model;
      formVoice = assistant.settings.voice;
      keyInput = "";
      tavilyKeyInput = "";
      keyError = "";
      void api.hasTavilyKey().then((has) => (hasSavedTavilyKey = has));
    }
  });

  $effect(() => {
    const provider = formProvider;
    void api.hasApiKey(provider).then((has) => {
      if (provider === formProvider) hasSavedKey = has;
    });
  });

  function onProviderChange(provider: AIProvider) {
    formProvider = provider;
    formModel = DEFAULT_MODELS[provider];
  }

  async function save(e: Event) {
    e.preventDefault();
    keyError = "";
    // Snapshot the inputs BEFORE any await: updateSettings() mutates
    // assistant.settings, which re-runs the re-seed $effect and would clear the
    // bound key fields mid-save (the writes below would then see empty strings).
    const provider = formProvider;
    const model = formModel.trim();
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
      if (!assistant.isConfigured) {
        keyError = "Enter an API key for this provider to use the assistant.";
        return;
      }
      keyInput = "";
      tavilyKeyInput = "";
      onClose();
    } catch (err) {
      keyError = String(err);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

{#if open}
  <div class="dialog-overlay" onclick={onClose} role="presentation">
    <div
      class="dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeyDown}
      role="dialog"
      tabindex="-1"
    >
      <div class="dialog-header">
        <h3 class="dialog-title">Settings</h3>
        <button class="dialog-close" onclick={onClose} title="Close">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      <form class="dialog-body" onsubmit={save}>
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
        </p>
        <p class="assistant-key-note">
          The assistant searches the web with Tavily when you toggle search on in the chat.
          Get a free key at app.tavily.com (1,000 searches/month).
        </p>
      </form>

      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn--secondary" onclick={onClose}>Cancel</button>
        <button class="dialog-btn dialog-btn--primary" onclick={save}>Save</button>
      </div>
    </div>
  </div>
{/if}
