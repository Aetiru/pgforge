<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import Skeleton from "./Skeleton.svelte";
  import { describeError, pickSave, schemaGraph, sqlWriteFile, type SchemaGraph } from "./ipc";
  import { notes } from "./notes.svelte";
  import { schemaMarkdown } from "./schema-doc";
  import type { SchemaDocTarget } from "./schema-doc.svelte";

  let { target, onclose }: { target: SchemaDocTarget; onclose: () => void } = $props();

  let graph = $state<SchemaGraph | null>(null);
  let error = $state<string | null>(null);
  let withDiagram = $state(true);
  let withNotes = $state(true);
  let copied = $state(false);
  let saved = $state<string | null>(null);

  // El esquema se lee una vez al abrir; para ver un cambio se vuelve a abrir el diálogo.
  $effect(() => {
    const { profileId, database, schema } = untrack(() => target);
    schemaGraph(profileId, database, schema)
      .then((loaded) => (graph = loaded))
      .catch((problem) => (error = describeError(problem)));
  });

  const markdown = $derived(
    graph
      ? schemaMarkdown(graph, {
          diagram: withDiagram,
          date: new Date().toISOString().slice(0, 10),
          noteOf: withNotes
            ? (table) => notes.get(target.profileId, target.database, target.schema, table.name)
            : undefined,
        })
      : "",
  );

  async function copy() {
    try {
      await navigator.clipboard.writeText(markdown);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Sin permiso del portapapeles el texto sigue a la vista para copiarlo a mano.
    }
  }

  async function save() {
    error = null;
    try {
      // La ruta sale de un diálogo abierto desde Rust: `sqlWriteFile` solo acepta esa.
      const path = await pickSave({
        title: "Guardar la documentación",
        defaultPath: `${target.database}-${target.schema}.md`,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!path) return;
      await sqlWriteFile(path, markdown);
      saved = path;
    } catch (problem) {
      error = describeError(problem);
    }
  }
</script>

<Modal title="Documentar el esquema" subtitle="{target.database} · {target.schema}" size="xl" {onclose}>
  <div class="flex flex-wrap items-center gap-x-5 gap-y-1 text-xs">
    <label class="check">
      <input type="checkbox" bind:checked={withDiagram} />
      Diagrama de relaciones (Mermaid)
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={withNotes} />
      Notas y etiquetas de esta computadora
    </label>
  </div>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {:else if !graph}
    <div class="mt-3"><Skeleton rows={8} /></div>
  {:else}
    <pre
      class="mt-3 max-h-[26rem] overflow-auto rounded-md border border-zinc-200 bg-zinc-50 p-3 font-mono
             text-xs whitespace-pre-wrap select-text dark:border-zinc-700 dark:bg-zinc-800/60"
      >{markdown}</pre>
    <p class="mt-2 text-xs muted">
      Sale de lo que hay en la base ahora, incluidos los comentarios de tablas y columnas. Para
      actualizarla, vuelve a generarla.
    </p>
  {/if}
  {#if saved}<Alert tone="ok" box class="mt-3 break-all">Guardado en {saved}.</Alert>{/if}

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cerrar</button>
    <button class="btn" disabled={!graph} onclick={copy}>{copied ? "Copiado ✓" : "Copiar"}</button>
    <button class="btn btn-primary" disabled={!graph} onclick={save}>
      <Icon name="download" size={12} />
      Guardar .md…
    </button>
  {/snippet}
</Modal>
