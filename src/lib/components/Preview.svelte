<script lang="ts">
  interface Props {
    htmlContent: string;
  }

  let { htmlContent }: Props = $props();
</script>

<div class="markdown-preview">
  <!-- eslint-disable-next-line svelte/no-at-html-tags — html comes from comrak with raw HTML escaped -->
  {@html htmlContent}
</div>
