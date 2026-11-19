/**
 * Una caché con vencimiento para lo que el editor le pregunta al servidor mientras se escribe
 * (valores de una columna, firmas de una función).
 *
 * Guarda la **promesa** y no el resultado: dos teclas seguidas piden lo mismo antes de que llegue
 * la primera respuesta, y sin esto cada una sería una vuelta al servidor. Un pedido que falla se
 * olvida enseguida: cachear un error dejaría la ayuda muerta hasta que venza.
 */
export class TtlCache<T> {
  private entries = new Map<string, { at: number; value: Promise<T> }>();

  constructor(
    private ttlMs: number,
    private now: () => number = Date.now,
    private max = 200,
  ) {}

  get(key: string, load: () => Promise<T>): Promise<T> {
    const hit = this.entries.get(key);
    if (hit && this.now() - hit.at < this.ttlMs) return hit.value;

    const value = load();
    this.entries.set(key, { at: this.now(), value });
    value.catch(() => {
      // Solo se borra si sigue siendo esta entrada: otra más nueva no se toca.
      if (this.entries.get(key)?.value === value) this.entries.delete(key);
    });
    if (this.entries.size > this.max) {
      // El más viejo primero: un `Map` conserva el orden de inserción.
      const oldest = this.entries.keys().next().value;
      if (oldest !== undefined) this.entries.delete(oldest);
    }
    return value;
  }

  clear() {
    this.entries.clear();
  }
}
