<script lang="ts" module>
  import type { FdwOption, OptionsDelta } from "./ipc";

  export interface OptionRow {
    key: string;
    value: string;
  }

  export function rowsFrom(options: FdwOption[]): OptionRow[] {
    return options.map(([key, value]) => ({ key, value }));
  }

  /** Las filas no vacías como pares [clave, valor], para un CREATE. */
  export function toOptions(rows: OptionRow[]): FdwOption[] {
    return rows
      .filter((row) => row.key.trim())
      .map((row) => [row.key.trim(), row.value] as FdwOption);
  }

  /**
   * El delta contra las opciones originales: altas (`add`), cambios de valor (`set`) y bajas
   * (`drop`). Postgres distingue ADD de SET, así que hay que separarlas comparando con el estado
   * anterior, no mandar todo como SET.
   */
  export function toDelta(original: FdwOption[], rows: OptionRow[]): OptionsDelta {
    const before = new Map(original.map(([key, value]) => [key, value]));
    const now = new Map(toOptions(rows));

    const add: FdwOption[] = [];
    const set: FdwOption[] = [];
    for (const [key, value] of now) {
      if (!before.has(key)) add.push([key, value]);
      else if (before.get(key) !== value) set.push([key, value]);
    }
    const drop = [...before.keys()].filter((key) => !now.has(key));
    return { add, set, drop };
  }

  export function deltaIsEmpty(delta: OptionsDelta): boolean {
    return delta.add.length === 0 && delta.set.length === 0 && delta.drop.length === 0;
  }
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";

  let { rows = $bindable() }: { rows: OptionRow[] } = $props();

  function add() {
    rows = [...rows, { key: "", value: "" }];
  }
  function remove(index: number) {
    rows = rows.filter((_, current) => current !== index);
  }
</script>

<div class="flex flex-col gap-1.5">
  {#each rows as row, index (index)}
    <div class="flex items-center gap-1.5">
      <input class="field flex-1" placeholder="clave" bind:value={row.key} />
      <input class="field flex-[2]" placeholder="valor" bind:value={row.value} />
      <button
        type="button"
        class="btn btn-ghost btn-icon shrink-0"
        title="Quitar la opción"
        aria-label="Quitar la opción"
        onclick={() => remove(index)}
      >
        <Icon name="close" size={12} />
      </button>
    </div>
  {/each}
  <button type="button" class="btn btn-ghost btn-sm self-start" onclick={add}>
    <Icon name="plus" size={11} />
    Agregar opción
  </button>
</div>
