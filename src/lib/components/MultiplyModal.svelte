<script lang="ts">
  import type { DocType } from "$lib/api";
  import { MULTIPLY_TARGETS, type MultiplyProgress } from "$lib/multiplyTargets";
  import DocumentIcon from "$lib/components/DocumentIcon.svelte";
  import Dialog from "$lib/components/Dialog.svelte";

  interface Props {
    open: boolean;
    sourceName: string;
    isConfigured: boolean;
    /** Non-null once a run is underway; one entry per chosen target. */
    progress: MultiplyProgress[] | null;
    onMultiply: (targets: { type: DocType; label: string }[]) => void;
    onCancel: () => void;
    onClose: () => void;
    onOpenSettings: () => void;
  }

  let { open, sourceName, isConfigured, progress, onMultiply, onCancel, onClose, onOpenSettings }: Props =
    $props();

  // Default every target checked — the common case is "give me all the versions".
  // Reseeded on each open so a previous partial selection doesn't leak through.
  let selected = $state<Set<DocType>>(new Set(MULTIPLY_TARGETS.map((t) => t.type)));

  $effect(() => {
    if (open) selected = new Set(MULTIPLY_TARGETS.map((t) => t.type));
  });

  const running = $derived(progress?.some((p) => p.status === "running") ?? false);
  const canClose = $derived(!running);

  function toggle(type: DocType) {
    const next = new Set(selected);
    if (next.has(type)) next.delete(type);
    else next.add(type);
    selected = next;
  }

  function start() {
    const targets = MULTIPLY_TARGETS.filter((t) => selected.has(t.type));
    if (targets.length === 0) return;
    onMultiply(targets);
  }

  function close() {
    if (canClose) onClose();
  }
</script>

<Dialog {open} title={`Multiply “${sourceName}”`} onClose={close} dismissible={canClose}>
  <div class="dialog-body">
        {#if !isConfigured}
          <p class="multiply-hint">Add an AI API key in Settings to multiply documents.</p>
          <button class="dialog-btn dialog-btn--primary" onclick={onOpenSettings}>
            Open Settings
          </button>
        {:else if progress}
          <p class="multiply-hint">Generating platform-native versions — one at a time.</p>
          <ul class="multiply-progress">
            {#each progress as p (p.type)}
              <li class="multiply-progress-row multiply-progress-row--{p.status}">
                <span class="multiply-progress-status">
                  {#if p.status === "running"}
                    <span class="multiply-spinner" title="Generating…"></span>
                  {:else if p.status === "done"}
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg>
                  {:else if p.status === "error"}
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></svg>
                  {:else}
                    <span class="multiply-dot"></span>
                  {/if}
                </span>
                <DocumentIcon type={p.type} size={16} />
                <span class="multiply-progress-label">{p.label}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <label class="dialog-label" for="multiply-targets">Generate versions for</label>
          <div class="multiply-targets" id="multiply-targets">
            {#each MULTIPLY_TARGETS as t (t.type)}
              <button
                class="multiply-target {selected.has(t.type) ? 'multiply-target--active' : ''}"
                onclick={() => toggle(t.type)}
                role="checkbox"
                aria-checked={selected.has(t.type)}
              >
                <span class="multiply-check">
                  {#if selected.has(t.type)}
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg>
                  {/if}
                </span>
                <DocumentIcon type={t.type} size={18} />
                <span class="multiply-target-label">{t.label}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>

  {#snippet footer()}
    <div class="dialog-footer">
        {#if progress}
          {#if running}
            <button class="dialog-btn dialog-btn--secondary" onclick={onCancel}>Cancel</button>
            <button class="dialog-btn dialog-btn--primary" disabled>Generating…</button>
          {:else}
            <button class="dialog-btn dialog-btn--primary" onclick={close}>Done</button>
          {/if}
        {:else}
          <button class="dialog-btn dialog-btn--secondary" onclick={close}>Cancel</button>
          <button
            class="dialog-btn dialog-btn--primary"
            onclick={start}
            disabled={!isConfigured || selected.size === 0}
          >
            Multiply{selected.size ? ` (${selected.size})` : ""}
          </button>
        {/if}
      </div>
  {/snippet}
</Dialog>

<style>
  .multiply-hint {
    margin: 0 0 0.75rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }
  .multiply-targets {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .multiply-target {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.6rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-secondary);
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
    font-size: 0.9rem;
    transition: border-color 0.12s, background 0.12s;
  }
  .multiply-target:hover {
    border-color: var(--accent);
  }
  .multiply-target--active {
    border-color: var(--accent);
    background: var(--bg-primary);
  }
  .multiply-check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--accent-contrast, #fff);
    background: transparent;
    flex-shrink: 0;
  }
  .multiply-target--active .multiply-check {
    background: var(--accent);
    border-color: var(--accent);
  }
  .multiply-target-label {
    font-weight: 500;
  }

  .multiply-progress {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .multiply-progress-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 0.9rem;
    color: var(--text-primary);
  }
  .multiply-progress-row--pending {
    opacity: 0.55;
  }
  .multiply-progress-row--error {
    border-color: var(--error, #e5484d);
    color: var(--error, #e5484d);
  }
  .multiply-progress-row--done .multiply-progress-status {
    color: var(--success, #30a46c);
  }
  .multiply-progress-status {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }
  .multiply-progress-label {
    font-weight: 500;
  }
  .multiply-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-secondary);
  }
  .multiply-spinner {
    width: 13px;
    height: 13px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
</style>
