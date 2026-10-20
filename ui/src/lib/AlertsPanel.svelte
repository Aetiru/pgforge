<script lang="ts">
  import Alert from "./Alert.svelte";
  import { ALERT_KEYS, RULE_SPECS, type AlertKey, type RuleStatus } from "./alerts";
  import { alerts } from "./alerts.svelte";
  import { decimal } from "./format";
  import Icon from "./Icon.svelte";

  let { profileId }: { profileId: string } = $props();

  const rules = $derived(alerts.rulesFor(profileId));

  const KIND_LABEL = { fired: "Saltó", repeat: "Sigue", resolved: "Resuelta" } as const;
  const KIND_TONE = {
    fired: "text-rose-600 dark:text-rose-400",
    repeat: "text-amber-600 dark:text-amber-400",
    resolved: "text-emerald-600 dark:text-emerald-400",
  } as const;

  function statusText(status: RuleStatus | undefined): { text: string; tone: string } {
    if (!status || status.kind === "off") return { text: "apagada", tone: "muted" };
    switch (status.kind) {
      case "ok":
        return {
          text: status.value === null ? "sin novedad" : `ahora ${decimal(status.value)}`,
          tone: "text-emerald-600 dark:text-emerald-400",
        };
      case "pending":
        return {
          text: `${decimal(status.value)}: esperando a que se sostenga`,
          tone: "text-amber-600 dark:text-amber-400",
        };
      case "firing":
        return { text: `encendida (${decimal(status.value)})`, tone: "text-rose-600 dark:text-rose-400" };
      case "restricted":
        return { text: status.reason, tone: "text-amber-600 dark:text-amber-400" };
      case "unknown":
        return { text: "sin dato todavía", tone: "muted" };
    }
  }

  function time(at: number): string {
    return new Date(at).toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  }

  const mine = $derived(alerts.history.filter((record) => record.profileId === profileId));
</script>

<div class="mx-4 mt-3 rounded-md border border-zinc-200 bg-white p-3 dark:border-zinc-700 dark:bg-zinc-800">
  <Alert tone="warn">
    Las alertas solo corren <strong>mientras este monitoreo está abierto</strong>. Si lo cerrás o
    cerrás la aplicación no se vigila nada, y nada avisa por detrás.
  </Alert>

  <table class="list-table mt-3 w-full text-xs">
    <thead>
      <tr>
        <th class="w-8"></th>
        <th class="text-left">Regla</th>
        <th class="text-left">Umbral</th>
        <th class="text-left">Ahora</th>
      </tr>
    </thead>
    <tbody>
      {#each ALERT_KEYS as key (key)}
        {@const spec = RULE_SPECS[key]}
        {@const rule = rules[key]}
        {@const status = statusText(alerts.status?.[key])}
        <tr>
          <td>
            <input
              type="checkbox"
              class="size-3.5 accent-blue-600"
              aria-label={`Activar la regla «${spec.label}»`}
              checked={rule.enabled}
              onchange={(event) =>
                alerts.setRule(profileId, key as AlertKey, { enabled: event.currentTarget.checked })}
            />
          </td>
          <td title={spec.description}>
            <div class="font-medium">{spec.label}</div>
            <div class="muted">{spec.description}</div>
          </td>
          <td class="whitespace-nowrap">
            <span class="muted">{spec.direction === "above" ? ">" : "<"}</span>
            <input
              type="number"
              class="field w-20 py-0.5 text-xs tabular-nums"
              min={spec.min}
              max={spec.max}
              step={spec.step}
              disabled={!rule.enabled}
              value={rule.threshold}
              aria-label={`Umbral de «${spec.label}», en ${spec.unit}`}
              onchange={(event) =>
                alerts.setRule(profileId, key as AlertKey, {
                  threshold: event.currentTarget.valueAsNumber,
                })}
            />
            <span class="muted">{spec.unit}</span>
          </td>
          <td class={status.tone}>{status.text}</td>
        </tr>
      {/each}
    </tbody>
  </table>

  <div class="mt-2 flex items-center gap-2">
    <button class="btn btn-sm btn-ghost" onclick={() => alerts.resetRules(profileId)}>
      Restablecer reglas
    </button>
    <span class="muted text-xs">
      Un solo aviso por alerta: no se repite hasta que se resuelve o pasan 30 minutos. El aviso del
      sistema usa el interruptor de la vista «Procesos».
    </span>
  </div>

  <div class="mt-3 flex items-center gap-2">
    <h3 class="text-xs font-semibold">Alertas recientes</h3>
    {#if mine.length > 0}
      <button class="btn btn-sm btn-ghost ml-auto" onclick={() => alerts.clearHistory()}>
        <Icon name="trash" size={11} />
        Vaciar
      </button>
    {/if}
  </div>
  {#if mine.length === 0}
    <p class="muted mt-1 text-xs">Todavía no saltó ninguna en este servidor.</p>
  {:else}
    <ul class="mt-1 flex flex-col gap-0.5 text-xs">
      {#each mine as record (record.id)}
        <li class="flex gap-2">
          <span class="muted tabular-nums">{time(record.at)}</span>
          <span class="w-16 font-semibold {KIND_TONE[record.kind]}">{KIND_LABEL[record.kind]}</span>
          <span class="font-medium">{record.title}</span>
          <span class="muted">{record.detail}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>
