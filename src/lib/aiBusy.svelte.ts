// ---------------------------------------------------------------------------
// Headless AI activity guard.
//
// A long, headless generation (content multiply or idea expand) discards its
// partial draft if the single backend AiState slot is taken by another stream
// mid-run. This flag marks such a run in progress so every *other* AI initiator
// — chat send, inline edit, and the other headless op — refuses to start and
// steal the slot. The two headless orchestrators (in +page.svelte) own the flag;
// everyone else only reads it.
// ---------------------------------------------------------------------------

class AiBusy {
  /** Human-readable label of the running headless generation, or null when idle. */
  label = $state<string | null>(null);

  get busy(): boolean {
    return this.label !== null;
  }

  begin(label: string) {
    this.label = label;
  }

  end() {
    this.label = null;
  }
}

export const aiBusy = new AiBusy();
