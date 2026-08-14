<script lang="ts">
  /**
   * Los botones de la cabecera del panel.
   *
   * Qué botones hay y en qué orden lo decide `headerActions`; acá solo se dibujan. Un botón apagado
   * lleva el motivo en el `title` —un botón apagado sin explicación se lee como una falla de la
   * aplicación y no como una decisión del perfil—, y por eso `blocked` se esparce después de él.
   */
  import Icon from "../Icon.svelte";
  import type { DetailAction } from "../detail-actions";

  let {
    actions,
    blocked = {},
    onaction,
  }: {
    actions: DetailAction[];
    blocked?: { disabled?: boolean; title?: string };
    onaction: (action: DetailAction) => void;
  } = $props();

  function toneClass(tone: DetailAction["tone"]): string {
    if (tone === "primary") return "btn-primary";
    if (tone === "danger") return "btn-danger-ghost";
    return "";
  }
</script>

<div class="ml-auto flex shrink-0 flex-wrap items-center justify-end gap-1.5">
  {#each actions as action (action.kind)}
    <button
      class="btn {toneClass(action.tone)} btn-icon"
      title={action.title}
      aria-label={action.label}
      {...action.guarded ? blocked : {}}
      onclick={() => onaction(action)}
    >
      <Icon name={action.icon} size={14} />
    </button>
  {/each}
</div>
