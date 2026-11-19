import { formatThousands } from "./number-format";

const KEY = "pgforge.thousands";

function read(): boolean {
  try {
    return localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

/** Si los números se muestran con separador de miles. Preferencia de la aplicación, como la zona. */
class NumberFormatPref {
  thousands = $state(read());

  toggle() {
    this.thousands = !this.thousands;
    try {
      localStorage.setItem(KEY, this.thousands ? "1" : "0");
    } catch {
      // Sin `localStorage` rige solo esta sesión.
    }
  }

  /** El texto de una celda numérica. */
  show(text: string): string {
    return this.thousands ? formatThousands(text) : text;
  }
}

export const numberFormat = new NumberFormatPref();
