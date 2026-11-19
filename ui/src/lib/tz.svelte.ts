import { convertTimestamptz, type TimeZoneMode } from "./tz";

const KEY = "pgforge.tz";

function read(): TimeZoneMode {
  try {
    const saved = localStorage.getItem(KEY);
    return saved === "local" || saved === "utc" ? saved : "server";
  } catch {
    return "server";
  }
}

/** En qué zona se muestran los `timestamptz`. Preferencia de la aplicación, como el tamaño de letra. */
class TimeZonePref {
  mode = $state<TimeZoneMode>(read());

  set(mode: TimeZoneMode) {
    this.mode = mode;
    try {
      localStorage.setItem(KEY, mode);
    } catch {
      // Sin `localStorage` rige solo esta sesión.
    }
  }

  /** El texto de una celda en la zona elegida. */
  show(text: string): string {
    return convertTimestamptz(text, this.mode);
  }
}

export const timeZone = new TimeZonePref();
