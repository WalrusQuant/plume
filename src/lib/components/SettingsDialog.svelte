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
  let keyInput = $state("");
  let keyError = $state("");
  let hasSavedKey = $state(false);

  // re-seed the form from saved settings each time the dialog opens
  $effect(() => {
    if (open) {
      formProvider = assistant.settings.provider;
      formModel = assistant.settings.model;
      keyInput = "";
      keyError = "";
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
    try {
      await assistant.updateSettings({ provider: formProvider, model: formModel.trim() });
      if (keyInput.trim()) {
        await assistant.saveKey(keyInput.trim());
      }
      if (!assistant.isConfigured) {
        keyError = "Enter an API key for this provider to use the assistant.";
        return;
      }
      keyInput = "";
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
      </form>

      <div class="dialog-footer">
        <button class="dialog-btn dialog-btn--secondary" onclick={onClose}>Cancel</button>
        <button class="dialog-btn dialog-btn--primary" onclick={save}>Save</button>
      </div>
    </div>
  </div>
{/if}
