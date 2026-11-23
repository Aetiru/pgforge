import { t } from "./i18n.svelte";
import { onServerDown } from "./ipc/core";
import { explorer } from "./explorer.svelte";
import { tabs } from "./tabs.svelte";
import { QueryTab } from "./query.svelte";
import { MAX_ATTEMPTS, delayBefore, givesUp } from "./reconnect-plan";

/**
 * Reconexión automática de un servidor caído.
 *
 * El árbol ya pinta el corte (`row.down`) y ofrece reconectar a mano; esto lo intenta solo, con
 * espera creciente, **sin pedir nada**: conecta con lo que el perfil ya tiene (contraseña
 * recordada o ninguna). Si el servidor pide una contraseña que no está guardada, el intento falla
 * por autenticación, se deja de insistir y queda el botón de siempre —preguntar una contraseña
 * sin que nadie haya hecho nada sería peor que el corte—.
 *
 * Al volver, las pestañas de consulta de ese servidor abren una sesión nueva: la anterior murió con
 * la conexión, y con ella lo que tuviera adentro (una transacción abierta, una tabla temporal). Se
 * avisa en cada pestaña; el texto escrito no se toca.
 */
class Reconnector {
  /** Intento en curso por servidor. */
  attempts = $state<Record<string, number>>({});
  private timers = new Map<string, ReturnType<typeof setTimeout>>();

  start(profileId: string) {
    if (this.timers.has(profileId) || this.attempts[profileId] !== undefined) return;
    this.schedule(profileId, 1);
  }

  /** Lo llama quien reconecta a mano: ya no hace falta esperar al siguiente intento. */
  stop(profileId: string) {
    const timer = this.timers.get(profileId);
    if (timer) clearTimeout(timer);
    this.timers.delete(profileId);
    const { [profileId]: _gone, ...rest } = this.attempts;
    this.attempts = rest;
  }

  private schedule(profileId: string, attempt: number) {
    const delay = delayBefore(attempt);
    if (delay === null) {
      this.stop(profileId);
      explorer.setDownNote(profileId, t("sin conexión"));
      return;
    }
    this.attempts = { ...this.attempts, [profileId]: attempt };
    explorer.setDownNote(profileId, t("reconectando… intento {attempt} de {MAX_ATTEMPTS}", { attempt, MAX_ATTEMPTS }));
    this.timers.set(
      profileId,
      setTimeout(() => void this.attempt(profileId, attempt), delay * 1000),
    );
  }

  private async attempt(profileId: string, attempt: number) {
    this.timers.delete(profileId);
    const profile = explorer.profiles.find((item) => item.id === profileId);
    // Si ya no está caído —se reconectó a mano— o el perfil desapareció, no hay nada que hacer.
    if (!profile || !explorer.isDown(profileId)) return this.stop(profileId);

    try {
      await explorer.connect(profile);
      this.stop(profileId);
      await this.reopenSessions(profileId);
    } catch (error) {
      if (givesUp(error)) {
        this.stop(profileId);
        explorer.setDownNote(profileId, t("sin conexión"));
        return;
      }
      this.schedule(profileId, attempt + 1);
    }
  }

  /** Abre una sesión nueva en cada pestaña de consulta del servidor que volvió. */
  private async reopenSessions(profileId: string) {
    const affected = tabs.all.filter(
      (tab): tab is QueryTab => tab instanceof QueryTab && tab.profileId === profileId,
    );
    await Promise.allSettled(affected.map((tab) => tab.reopenSession()));
  }
}

export const reconnector = new Reconnector();

onServerDown((profileId) => reconnector.start(profileId));
