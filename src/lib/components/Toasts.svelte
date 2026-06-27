<script lang="ts">
  import { toast } from "$lib/toast.svelte";
</script>

{#if toast.toasts.length > 0}
  <!-- aria-live announces new toasts to screen readers; role=alert on error
       toasts makes them announce immediately. -->
  <div class="toast-stack" aria-live="polite" aria-atomic="false">
    {#each toast.toasts as t (t.id)}
      <div class="toast toast--{t.kind}" role={t.kind === "error" ? "alert" : "status"}>
        <span class="toast-message">{t.message}</span>
        <button class="toast-dismiss" onclick={() => toast.dismiss(t.id)} aria-label="Dismiss" title="Dismiss">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>
    {/each}
  </div>
{/if}
