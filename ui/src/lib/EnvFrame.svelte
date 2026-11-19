<script lang="ts">
  import { envFrame } from "./badges";
  import type { Environment } from "./ipc";

  let { environment, server }: { environment: Environment | null; server: string } = $props();

  const look = $derived(envFrame(environment));
</script>

<!--
  Superpuesto a toda la ventana y con `pointer-events-none`: no ocupa lugar ni se come clics. Es una
  franja fina arriba y no un marco de cuatro lados: producción se señalaba en cinco lugares a la vez
  y, cuando todo es rojo, nada destaca. Va por
  encima de los diálogos (z-[60]) a propósito: la confirmación de una operación en producción es
  justo donde más hace falta saber en qué servidor se está parado. La cinta nombra el servidor solo
  en producción; en dev y test alcanza el marco fino. Va abajo y al centro, sobre el hueco de la
  barra de estado: arriba tapaba los botones de la tira de pestañas.
-->
{#if look}
  <div class="pointer-events-none fixed inset-0 z-[60] {look.frame}" aria-hidden="true">
    {#if environment === "prod"}
      <span
        class="absolute bottom-0.5 left-1/2 flex -translate-x-1/2 items-center gap-1.5 rounded-md
               px-2.5 py-px text-[10px] font-semibold tracking-widest uppercase {look.ribbon}"
      >
        <span class="size-1.5 rounded-full bg-current"></span>
        Producción{server ? ` · ${server}` : ""}
      </span>
    {/if}
  </div>
{/if}
