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
    onopenfile,
    onmenu,
  }: {
    folder: ScriptFolder;
    depth: number;
    /** La carpeta de primer nivel, una por conexión: no tiene acción de abrir, solo de contraer. */
    isConnection?: boolean;
    onopenfile: (file: ScriptEntry) => void;
    onmenu: (event: MouseEvent, target: MenuTarget) => void;
  } = $props();

  const open = $derived(scripts.isOpen(folder.path));
  const indent = $derived(8 + depth * 14);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="flex h-6 items-center gap-1.5 rounded px-1 text-sm hover:bg-zinc-100
         dark:hover:bg-zinc-700/70"
  style="padding-left: {indent}px"
  oncontextmenu={(event) => onmenu(event, { kind: "folder", folder, isConnection })}
>
  <button
    class="grid size-4 shrink-0 place-items-center rounded text-zinc-400
           hover:text-zinc-900 dark:hover:text-zinc-100"
    aria-label={open ? "Contraer" : "Expandir"}
    onclick={() => scripts.setOpen(folder.path, !open)}
  >
    <Icon name="chevron" size={12} class="transition-transform {open ? 'rotate-90' : ''}" />
  </button>
  <Icon name="folder" size={13} class="shrink-0 text-amber-600 dark:text-amber-400" />
  <span
    class="min-w-0 flex-1 truncate {isConnection ? 'font-medium' : ''}"
    title={folder.path}
  >
    {folder.name}
  </span>
</div>

{#if open}
  {#each folder.folders as sub (sub.path)}
    <ScriptFolderRow folder={sub} depth={depth + 1} {onopenfile} {onmenu} />
  {/each}

  {#each folder.files as file (file.path)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex h-6 items-center gap-1.5 rounded px-1 text-sm hover:bg-zinc-100
             dark:hover:bg-zinc-700/70"
      style="padding-left: {indent + 18}px"
      oncontextmenu={(event) => onmenu(event, { kind: "file", file, folder })}
    >
      <button
        class="flex min-w-0 flex-1 items-center gap-1.5 text-left"
        title={file.path}
        onclick={() => onopenfile(file)}
      >
        <Icon name="sql" size={13} class="shrink-0 text-violet-600 dark:text-violet-400" />
        <span class="min-w-0 flex-1 truncate">{file.name}</span>
      </button>
    </div>
  {/each}
{/if}
