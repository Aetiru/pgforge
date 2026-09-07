<script lang="ts" module>
  import type { ScriptEntry, ScriptFolder } from "./ipc";

  export type MenuTarget =
    | { kind: "folder"; folder: ScriptFolder; isConnection: boolean }
    | { kind: "file"; file: ScriptEntry; folder: ScriptFolder };
</script>

<script lang="ts">
  /**
   * Una fila del árbol de scripts, recursiva: se dibuja a sí misma para cada subcarpeta.
   *
   * No hay virtualización como en `TreePanel`: un árbol de scripts son decenas de archivos como
   * mucho, muy lejos de las miles de filas que la justifican ahí.
   */
  import Icon from "./Icon.svelte";
  // Auto-importado: `<svelte:self>` está obsoleto en Svelte 5 a favor de que un componente se
  // importe a sí mismo para recursar, que es justo lo que hace falta para una subcarpeta dentro de
  // otra.
  import ScriptFolderRow from "./ScriptFolderRow.svelte";
  import { scripts } from "./scripts.svelte";

  let {
    folder,
    depth,
    isConnection = false,
    separated = false,
    onopenfile,
    onmenu,
  }: {
    folder: ScriptFolder;
    depth: number;
    /** La carpeta de primer nivel, una por conexión: no tiene acción de abrir, solo de contraer. */
    isConnection?: boolean;
    /** Lleva línea divisoria arriba: todas las conexiones salvo la primera de la lista. */
    separated?: boolean;
    onopenfile: (file: ScriptEntry) => void;
    onmenu: (event: MouseEvent, target: MenuTarget) => void;
  } = $props();

  const open = $derived(scripts.isOpen(folder.path));
  const indent = $derived(8 + depth * 14);

  /** Cuántos `.sql` hay adentro, contando los de las subcarpetas: lo que dice si vale la pena
   * abrirla sin tener que hacerlo. */
  function countAll(node: ScriptFolder): number {
    return node.files.length + node.folders.reduce((sum, sub) => sum + countAll(sub), 0);
  }
  const count = $derived(countAll(folder));
</script>

<!--
  La carpeta de conexión se dibuja como **sección** —versalita, sin ícono de carpeta— igual que las
  carpetas del catálogo en el árbol de servidores (`isSection` de `TreePanel`): agrupar se nota por
  la tipografía, no por un ícono repetido en cada conexión. Lleva además línea divisoria arriba
  salvo la primera, para que dos conexiones abiertas a la vez no se lean como una sola lista.
-->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="relative flex items-center gap-1.5 px-1 hover:bg-zinc-100 dark:hover:bg-zinc-700/70
         {isConnection
    ? 'h-[26px] text-[11px] font-semibold tracking-wide uppercase muted'
    : 'h-6 rounded text-sm'}
         {isConnection && separated ? 'border-t border-zinc-200/80 dark:border-zinc-700' : ''}"
  style="padding-left: {indent}px"
  onclick={() => scripts.setOpen(folder.path, !open)}
  oncontextmenu={(event) => onmenu(event, { kind: "folder", folder, isConnection })}
>
  <!-- Guías de sangría: una línea vertical por nivel, igual que en el árbol de servidores, para que
       de qué carpeta cuelga un archivo se lea sin contar sangrías con el dedo. -->
  {#each { length: depth }, level (level)}
    <span
      class="pointer-events-none absolute inset-y-0 w-px bg-zinc-200 dark:bg-zinc-700"
      style="left: {8 + level * 14 + 7}px"
    ></span>
  {/each}

  <button
    class="relative grid size-4 shrink-0 place-items-center rounded text-zinc-400
           hover:text-zinc-900 dark:hover:text-zinc-100"
    aria-label={open ? "Contraer" : "Expandir"}
    onclick={(event) => {
      event.stopPropagation();
      scripts.setOpen(folder.path, !open);
    }}
  >
    <Icon name="chevron" size={12} class="transition-transform {open ? 'rotate-90' : ''}" />
  </button>
  {#if !isConnection}
    <Icon name="folder" size={13} class="relative shrink-0 text-amber-600 dark:text-amber-400" />
  {/if}
  <span class="relative min-w-0 flex-1 truncate" title={folder.path}>
    {folder.name}
  </span>
  {#if count > 0}
    <span class="seg-count relative tabular-nums">{count}</span>
  {/if}
</div>

{#if open}
  {#each folder.folders as sub (sub.path)}
    <ScriptFolderRow folder={sub} depth={depth + 1} {onopenfile} {onmenu} />
  {/each}

  {#each folder.files as file (file.path)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="relative flex h-6 items-center gap-1.5 rounded px-1 text-sm hover:bg-zinc-100
             dark:hover:bg-zinc-700/70"
      style="padding-left: {indent + 18}px"
      oncontextmenu={(event) => onmenu(event, { kind: "file", file, folder })}
    >
      {#each { length: depth + 1 }, level (level)}
        <span
          class="pointer-events-none absolute inset-y-0 w-px bg-zinc-200 dark:bg-zinc-700"
          style="left: {8 + level * 14 + 7}px"
        ></span>
      {/each}
      <button
        class="relative flex min-w-0 flex-1 items-center gap-1.5 text-left"
        title={file.path}
        onclick={() => onopenfile(file)}
      >
        <Icon name="sql" size={13} class="shrink-0 text-violet-600 dark:text-violet-400" />
        <span class="min-w-0 flex-1 truncate text-[12px]">{file.name}</span>
      </button>
    </div>
  {/each}
{/if}
