<script lang="ts">
  import Icon from "./Icon.svelte";
  import Sql from "./Sql.svelte";
  import { CUSTOM_ID, PALETTES, SQL_TOKENS, colorsOf, type KeywordWeight } from "./sql-palettes";
  import { sqlTheme } from "./sql-theme.svelte";

  /**
   * Los colores del editor SQL. Cada cambio se aplica en el acto y el ejemplo de abajo es el mismo
   * `Sql` de solo lectura que muestra el DDL de un objeto: lo que se ve acá es lo que se va a ver en el
   * editor, con las mismas variables, y no una maqueta aparte.
   */
  const SAMPLE = `-- Ventas por cliente
SELECT c.nombre, sum(p.total)::numeric(12,2) AS vendido
  FROM pedidos p
  JOIN clientes c ON c.id = p.cliente_id
 WHERE p.estado <> 'cancelado' AND p.total > 1500.50
 GROUP BY c.nombre
 LIMIT 10;`;

  const WEIGHTS: { value: KeywordWeight; label: string }[] = [
    { value: 400, label: "Normal" },
    { value: 500, label: "Media" },
    { value: 700, label: "Negrita" },
  ];

  const prefs = $derived(sqlTheme.prefs);
  // El selector de color es del tema que se está mirando; el otro se edita aparte.
  let editing = $state<"light" | "dark">("light");
  const swatch = (id: string, theme: "light" | "dark") =>
    id === CUSTOM_ID ? prefs.custom[theme] : (PALETTES.find((p) => p.id === id) ?? PALETTES[0])[theme];
</script>

<div class="flex flex-col gap-3">
  <div class="flex items-center justify-between">
    <span class="label">Colores del SQL</span>
    <button class="btn btn-sm btn-ghost" onclick={() => sqlTheme.reset()} title="Vuelve a la paleta y los ajustes de fábrica">
      Restablecer
    </button>
  </div>

  <div class="grid grid-cols-2 gap-2 @md:grid-cols-3">
    {#each [...PALETTES, { id: CUSTOM_ID, name: "Personalizada", description: "Eliges cada color." }] as palette (palette.id)}
      {@const active = prefs.palette === palette.id}
      <button
        class="card flex flex-col items-start gap-1.5 px-3 py-2 text-left transition-colors
          hover:border-blue-400 dark:hover:border-blue-500
          {active ? 'border-blue-500 ring-2 ring-blue-500/20 dark:border-blue-500' : ''}"
        aria-pressed={active}
        title={palette.description}
        onclick={() => sqlTheme.setPalette(palette.id)}
      >
        <span class="flex w-full items-center justify-between">
          <span class="text-sm font-medium">{palette.name}</span>
          {#if active}
            <Icon name="check" size={13} class="text-blue-600 dark:text-blue-400" />
          {/if}
        </span>
        <span class="flex gap-0.5">
          {#each ["keyword", "string", "number", "type", "function", "comment"] as const as token (token)}
            <span class="size-3.5 rounded-sm border border-black/10 dark:border-white/10" style="background: {swatch(palette.id, 'light')[token]}"></span>
          {/each}
        </span>
        <span class="flex gap-0.5">
          {#each ["keyword", "string", "number", "type", "function", "comment"] as const as token (token)}
            <span class="size-3.5 rounded-sm border border-black/10 dark:border-white/10" style="background: {swatch(palette.id, 'dark')[token]}"></span>
          {/each}
        </span>
      </button>
    {/each}
  </div>

  <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
    <div class="flex items-center gap-2 text-sm">
      Palabras clave
      <div class="seg" role="group" aria-label="Peso de las palabras clave">
        {#each WEIGHTS as weight (weight.value)}
          <button
            class="seg-item"
            aria-pressed={prefs.keywordWeight === weight.value}
            onclick={() => sqlTheme.setWeight(weight.value)}
          >
            {weight.label}
          </button>
        {/each}
      </div>
    </div>
    <label class="check text-sm">
      <input type="checkbox" checked={prefs.commentItalic} onchange={(event) => sqlTheme.setCommentItalic(event.currentTarget.checked)} />
      Comentarios en cursiva
    </label>
    <label class="check text-sm" title="Pinta las tablas y columnas con su propio color en vez del del texto">
      <input type="checkbox" checked={prefs.identTint} onchange={(event) => sqlTheme.setIdentTint(event.currentTarget.checked)} />
      Distinguir nombres de tablas y columnas
    </label>
  </div>

  {#if prefs.palette === CUSTOM_ID}
    <div class="flex flex-col gap-2 rounded-md bg-zinc-50 p-2.5 dark:bg-zinc-900/60">
      <div class="seg w-fit" role="group" aria-label="Tema que se edita">
        <button class="seg-item" aria-pressed={editing === "light"} onclick={() => (editing = "light")}>Colores del claro</button>
        <button class="seg-item" aria-pressed={editing === "dark"} onclick={() => (editing = "dark")}>Colores del oscuro</button>
      </div>
      <div class="grid grid-cols-2 gap-x-4 gap-y-1.5">
        {#each SQL_TOKENS as token (token.key)}
          <label class="flex items-center gap-2 text-xs">
            <input
              type="color"
              class="h-5 w-8 cursor-pointer rounded border border-zinc-300 bg-transparent p-0 dark:border-zinc-600"
              value={colorsOf(prefs, editing)[token.key]}
              oninput={(event) => sqlTheme.setColor(editing, token.key, event.currentTarget.value)}
            />
            {token.label}
          </label>
        {/each}
      </div>
      <p class="text-[11px] muted">Para ver el otro tema, cambia el tema de la aplicación desde la barra lateral.</p>
    </div>
  {/if}

  <div class="rounded-md border border-zinc-200 bg-white px-3 py-2 select-text dark:border-zinc-700 dark:bg-zinc-900">
    <Sql code={SAMPLE} />
  </div>
</div>
