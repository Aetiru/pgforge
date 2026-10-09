<script lang="ts">
  import { envFrame } from "./badges";
  import type { Environment } from "./ipc";

  let { environment, server }: { environment: Environment | null; server: string } = $props();

  const look = $derived(envFrame(environment));
</script>

<!--
  Superpuesto a toda la ventana y con `pointer-events-none`: no ocupa lugar ni se come clics. Va por
  encima de los diálogos (z-[60]) a propósito: la confirmación de una operación en producción es
  justo donde más hace falta saber en qué servidor se está parado. La cinta nombra el servidor solo
  en producción; en dev y test alcanza el marco fino.
-->
{#if look}
  <div class="pointer-events-none fixed inset-0 z-[60] {look.frame}" aria-hidden="true">
    {#if environment === "prod"}
      <span
        class="absolute top-0 left-1/2 -translate-x-1/2 rounded-b-md px-3 py-px text-[10px]
               font-semibold tracking-widest uppercase {look.ribbon}"
      >
        Producción{server ? ` · ${server}` : ""}
      </span>
    {/if}
  </div>
{/if}
