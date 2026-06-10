export interface Toast {
  id: number;
  message: string;
  kind: "error" | "info";
}

class ToastStore {
  toasts = $state<Toast[]>([]);
  private nextId = 1;

  show(message: string, kind: Toast["kind"] = "error") {
    const id = this.nextId++;
    this.toasts = [...this.toasts, { id, message, kind }];
    setTimeout(() => this.dismiss(id), 6000);
  }

  error(message: string) {
    this.show(message, "error");
  }

  dismiss(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }
}

export const toast = new ToastStore();
