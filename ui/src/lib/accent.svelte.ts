/**
 * El color de acento de la interfaz.
 *
 * Cambia solo el azul de marca —pestaña activa, selección, enlaces, anillo de foco—. Los colores de
 * estado (producción en rojo, transacción en ámbar, ejecutar en verde) no dependen de esto y no
 * cambian: un acento rojo no puede hacerse pasar por el aviso de producción. El valor se escribe en
 * `data-accent` del documento y `app.css` reasigna la paleta `blue` a partir de ahí.
 *
 * Sin `wstorage`, igual que el tema: es preferencia de la aplicación y vale para todas las ventanas.
 */
import { ACCENTS, parseAccent, type Accent } from "./accent";

export { ACCENTS };

const KEY = "pgforge.accent";

function stored(): Accent {
  try {
    return parseAccent(localStorage.getItem(KEY));
  } catch {
    return "blue";
  }
}

class AccentPreference {
  choice = $state<Accent>(stored());

  constructor() {
    this.apply();
  }

  private apply() {
    const root = document.documentElement;
    if (this.choice === "blue") root.removeAttribute("data-accent");
    else root.setAttribute("data-accent", this.choice);
  }

  set(choice: Accent) {
    this.choice = choice;
    try {
      localStorage.setItem(KEY, choice);
    } catch {
      // Sin almacenamiento, vale hasta cerrar la ventana.
    }
    this.apply();
  }
}

export const accent = new AccentPreference();
