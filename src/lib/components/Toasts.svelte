<script lang="ts">
  import { toast } from "$lib/toast.svelte";
</script>

{#if toast.toasts.length > 0}
  <div class="toast-stack">
    {#each toast.toasts as t (t.id)}
      <button class="toast toast--{t.kind}" onclick={() => toast.dismiss(t.id)} title="Dismiss">
        {t.message}
      </button>
    {/each}
  </div>
{/if}
