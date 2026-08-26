/**
 * Aviso in-app de que un proceso terminó, al lado del aviso del sistema que manda `notify`
 * (`notify.svelte.ts`). El del sistema puede no verse —la ventana puede estar minimizada o el
 * usuario en otra ventana—; este es la versión que sí se ve si la aplicación está a la vista pero
 * en otra pantalla que no es la de procesos.
 */

export interface ToastItem {
  id: string;
  tone: "ok" | "bad";
  title: string;
  body: string;
}

const DURATION_MS = 6000;

class Toasts {
  items = $state<ToastItem[]>([]);

  push(tone: ToastItem["tone"], title: string, body: string) {
    const id = crypto.randomUUID();
    this.items = [...this.items, { id, tone, title, body }];
    setTimeout(() => this.dismiss(id), DURATION_MS);
  }

  dismiss(id: string) {
    this.items = this.items.filter((item) => item.id !== id);
  }
}

export const toasts = new Toasts();
